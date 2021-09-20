use anyhow::{anyhow, Result};
use libc::{c_void, recv, MSG_TRUNC};
use std::{
    io::{self, Write},
    net::TcpStream,
    os::unix::prelude::AsRawFd,
    sync::atomic::{AtomicU64, Ordering},
    thread::{sleep, spawn},
    time::{Duration, Instant},
};
use structopt::StructOpt;

const REQUEST: &[u8] =
    b"GET / HTTP/1.0\r\nX-Email-Id: iam@lach.pw\r\nConnection: keep-alive\r\n\r\n";

static REQS_TOTAL: AtomicU64 = AtomicU64::new(0);
static REQS: AtomicU64 = AtomicU64::new(0);
static SKIPPED: AtomicU64 = AtomicU64::new(0);
static SENT: AtomicU64 = AtomicU64::new(0);

#[derive(StructOpt)]
#[structopt(author = "Yaroslav Bolyukin <iam@lach.pw>")]
struct Opts {
    /// Hosts to use, resolve rhsbin.tech to get them
    #[structopt(long)]
    hosts: Vec<String>,
    /// How many connections should be opened to every host
    #[structopt(long)]
    batches: usize,
    /// How many requests should be joined to one bulk
    ///
    /// If too low - then this program won't be io-bounded,
    /// if too high - stats will be inaccurate
    #[structopt(long)]
    reqs: usize,
}

fn handle_host(str: &str, reqs: usize, connected: &mut bool) -> Result<()> {
    let mut stream = TcpStream::connect(str)?;
    stream.set_read_timeout(Some(Duration::from_secs(16)))?;
    stream.set_write_timeout(Some(Duration::from_secs(16)))?;

    *connected = true;

    let fd = stream.as_raw_fd();
    let recv = spawn(move || -> Result<()> {
        let bufsize = 65535;
        let mut buf = vec![0; bufsize];

        loop {
            let skipped =
                unsafe { recv(fd, &mut buf as *mut _ as *mut c_void, bufsize, MSG_TRUNC) };
            if skipped == -1 {
                return Err(anyhow!("Error: {}", io::Error::last_os_error()));
            }
            SKIPPED.fetch_add(skipped as u64, Ordering::SeqCst);
        }
    });
    spawn(move || -> Result<(), io::Error> {
        let mut buf = Vec::<u8>::new();

        let reqs_per_buf = reqs;
        for _ in 0..reqs_per_buf {
            buf.extend(REQUEST.iter());
        }
        loop {
            stream.write_all(&buf)?;
            SENT.fetch_add(buf.len() as u64, Ordering::SeqCst);
            REQS.fetch_add(reqs_per_buf as u64, Ordering::SeqCst);
            REQS_TOTAL.fetch_add(reqs_per_buf as u64, Ordering::SeqCst);
        }
    })
    .join()
    .map_err(|_| anyhow!("Send failed"))??;
    recv.join().map_err(|_| anyhow!("Recv failed"))??;

    Ok(())
}

fn thread_entry(str: &str, reqs: usize) {
    let initial = Duration::from_secs(1);
    let mut time = initial;
    let max = Duration::from_secs(16);
    loop {
        eprintln!("Connecting to {}", str);
        let mut connected = false;
        let started = Instant::now();
        if let Err(e) = handle_host(str, reqs, &mut connected) {
            eprintln!("Connection errored = {:?}", e);
        } else {
            eprintln!("Disconnected D:");
        }

        let succeeded = connected && Instant::now() - started >= Duration::from_secs(1);

        if succeeded {
            time = initial;
        }
        eprintln!("Waiting for {:?}...", time);
        std::thread::sleep(time);
        if !succeeded {
            time *= 2;
            if time > max {
                time = max;
            }
        }
    }
}

fn main() {
    let opts = Opts::from_args();

    let mut threads = Vec::new();
    for _ in 0..opts.batches {
        for host in &opts.hosts {
            let host = host.clone();
            let reqs = opts.reqs;
            threads.push(spawn(move || thread_entry(&host, reqs)));
        }
    }
    threads.push(spawn(|| loop {
        let sent = SENT.swap(0, Ordering::SeqCst);
        let skipped = SKIPPED.swap(0, Ordering::SeqCst);
        let reqs = REQS.swap(0, Ordering::SeqCst);
        let total = REQS_TOTAL.load(Ordering::SeqCst);

        println!(
            "TX: {: >10}/s RX: {: >10}/s REQS: {: >10}/s TOTAL: {}",
            sent, skipped, reqs, total,
        );

        sleep(Duration::from_secs(1));
    }));

    for thread in threads {
        thread.join().unwrap();
    }
}
