extern crate core;
extern crate hyper;
extern crate tokio;
extern crate futures;
extern crate hyper_tls;
extern crate regex;
extern crate reqwest;

use std::rc::Rc;
use std::cell::RefCell;
use std::env;
use std::sync::{Arc, Mutex, Condvar};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::atomic::Ordering;
use std::thread;

use core::time;

use tokio::runtime::Runtime;
use tokio::prelude::future::FutureResult;
use futures::{future, Future};
use hyper::StatusCode;
use hyper::rt::{self};
use std::result::Result::Ok;
use futures::Poll;
use tokio::prelude::IntoFuture;
use futures::Stream;
use std::sync::mpsc;
use regex::Regex;
use std::fs::read_to_string;
use reqwest::async::{Client, Decoder};
use std::mem;
use std::io::Cursor;
use std::io;
use std::str;

fn process_url(url: String, processing_units: Arc<AtomicUsize>, client: Client, tx: mpsc::Sender<Vec<String>>) -> impl Future<Item=(), Error=()> {
    client
        .get(&url)
        .send()
        .then(move |res| {
            let mut res = res.unwrap();
            let body = mem::replace(res.body_mut(), Decoder::empty());
            body.concat2()
                .map(move |body| {
                    let mut body = Cursor::new(body);
                    let mut writer: Vec<u8> = vec![];
                    let _ = io::copy(&mut body, &mut writer).map_err(|err| {
                        println!("stdout error: {}", err);
                    });

                    let body = str::from_utf8(&writer).unwrap();

                    let mut links;

                    if res.status() == StatusCode::MOVED_PERMANENTLY {
                        let redirection = res.headers().get("Location").unwrap().to_str().unwrap();
                        links = vec![redirection.to_string()];
                    } else {
                        let re = Regex::new(r#"(href|src)=["']([\w/.:\-\d,?=]+)["']"#).unwrap();

                        links = vec![];
                        for caps in re.captures_iter(body) {
                            links.push(caps[2].to_string());
                        }
                    }
                    processing_units.fetch_sub(1, Ordering::SeqCst); // Must be before the tx send
                    tx.send(links);
                })
                .then(|_| {
                    future::ok(())
                })
        })
}

fn main() {
    let url = match env::args().nth(1) {
        Some(url) => url,
        None => {
            println!("Usage: client <url>");
            return;
        }
    };

    let processing_units = Arc::new(AtomicUsize::new(0));

    let mut rt = Runtime::new().unwrap();

    let client = Client::new();

    let (tx, rx) = mpsc::channel();

    {
        processing_units.fetch_add(1, Ordering::SeqCst);
        let processing_units = Arc::clone(&processing_units);
        let client = client.clone();
        let tx = mpsc::Sender::clone(&tx);
        let url = url.clone();
        rt.spawn(future::lazy(|| process_url(url, processing_units, client, tx)));
    }

    for received in rx {
        println!("Received links !");
        for mut link in received {
            {
                println!("{}", link);
                let tx = mpsc::Sender::clone(&tx);
                processing_units.fetch_add(1, Ordering::SeqCst);
                let processing_units = Arc::clone(&processing_units);
                let client = client.clone();
                if link.get(0..1).unwrap() == "/" {
                    link = format!("{}{}", url, link);
                }
                rt.spawn(future::lazy(|| process_url(link, processing_units, client, tx)));
            }
        }

        if processing_units.load(Ordering::SeqCst) == 0 {
            break;
        }
    }

    println!("Finished checking website.");
}
