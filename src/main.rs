extern crate core;
extern crate hyper;
extern crate tokio;
extern crate futures;
extern crate hyper_tls;
extern crate regex;

use std::rc::Rc;
use std::cell::RefCell;
use std::env;
use std::sync::{Arc, Mutex, Condvar};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::atomic::Ordering;
use std::thread;

use core::time;

use hyper::Uri;
use tokio::runtime::Runtime;
use tokio::prelude::future::FutureResult;
use futures::{future, Future};
use hyper::{Client, StatusCode, Response};
use hyper_tls::HttpsConnector;
use hyper::client::HttpConnector;
use hyper::rt::{self};
use std::result::Result::Ok;
use futures::Poll;
use tokio::prelude::IntoFuture;
use futures::Stream;
use tokio::io;
use hyper::client::ResponseFuture;
use std::sync::mpsc;
use regex::Regex;
use std::fs::read_to_string;

fn process_url(url: Uri, processing_units: Arc<AtomicUsize>, client: Client<HttpsConnector<HttpConnector>>, tx: mpsc::Sender<Vec<Uri>>) -> impl Future<Item=(), Error=()> {
    client
        .get(url)
        .then(move |res|{
            let res = res.unwrap();
            if res.status() == StatusCode::MOVED_PERMANENTLY {
                let redirection =  res.headers().get("Location").unwrap().to_str().unwrap();
                tx.send(vec![redirection.parse::<Uri>().unwrap()]);

            } else {
                println!("{}", res.status());
                let re = Regex::new(r#"(href|src)=["']([\w/.:\-\d,?=]+)["']"#).unwrap();
                let body = String::from(res.into_body()).to_str();

                for cap in re.captures_iter(body) {
                    println!("{}", cap);
                }
                tx.send(vec![]);
            }

            processing_units.fetch_sub(1, Ordering::SeqCst);
            future::ok(())
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

    let url = url.parse::<Uri>().unwrap();
    let processing_units = Arc::new(AtomicUsize::new(0));

    let mut rt = Runtime::new().unwrap();

    let https = HttpsConnector::new(4).unwrap();
    let client = Client::builder()
        .build::<_, hyper::Body>(https);

    let (tx, rx) = mpsc::channel();

    {
        processing_units.fetch_add(1, Ordering::SeqCst);
        let processing_units = Arc::clone(&processing_units);
        let client = client.clone();
        let tx = mpsc::Sender::clone(&tx);
        rt.spawn(future::lazy(|| process_url(url, processing_units, client, tx)));
    }

    for received in rx {
        for link in received {
            {
                let tx = mpsc::Sender::clone(&tx);
                processing_units.fetch_add(1, Ordering::SeqCst);
                let processing_units = Arc::clone(&processing_units);
                let client = client.clone();
                rt.spawn(future::lazy(|| process_url(link, processing_units, client, tx)));
            }
        }

        if processing_units.load(Ordering::SeqCst) == 0 {
            break;
        }
    }

    println!("Finished checking website.");
}
