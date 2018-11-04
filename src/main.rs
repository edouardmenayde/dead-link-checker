extern crate core;
extern crate hyper;
extern crate tokio;
extern crate futures;
extern crate hyper_tls;

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

fn process_url(url: Uri, processing_units: Arc<AtomicUsize>, is_empty: Arc<Condvar>, client: Client<HttpsConnector<HttpConnector>>) -> impl Future<Item=(), Error=()> {
    println!("Processed {}", url);
    client
        .get(url)
        .then(move |res|{
            let res = res.unwrap();
            if res.status() == StatusCode::MOVED_PERMANENTLY {
                println!("{}", res.headers().get("Location").unwrap().to_str().unwrap());
            } else {
                println!("{}", res.status());
            }

            println!("\n\nFinished checking website.");
            processing_units.fetch_sub(1, Ordering::SeqCst);
            is_empty.notify_one();

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

    let links: Rc<RefCell<Vec<Uri>>> = Rc::new(RefCell::new(Vec::new()));

    links.borrow_mut().push(url.clone());

    let mutex = Arc::new(Mutex::new(false));
    let is_empty = Arc::new(Condvar::new());
    let has_finished_checking = Arc::new(AtomicBool::new(false));
    let processing_units = Arc::new(AtomicUsize::new(0));

    let mut rt = Runtime::new().unwrap();

    let https = HttpsConnector::new(4).unwrap();
    let client = Client::builder()
        .build::<_, hyper::Body>(https);

    loop {
        println!("Looping");
        let link;

        {
            let mutex_guard = mutex.lock().unwrap();

            if processing_units.load(Ordering::SeqCst) == 0 && links.borrow().is_empty() {
                has_finished_checking.store(true, Ordering::SeqCst);
            }

            if has_finished_checking.load(Ordering::SeqCst) {
                println!("Finished");
                break;
            }

            if links.borrow().is_empty() {
                println!("Empty");
                is_empty.wait(mutex_guard).unwrap();
            }

            if processing_units.load(Ordering::SeqCst) == 0 && links.borrow().is_empty() {
                has_finished_checking.store(true, Ordering::SeqCst);
            }

            if has_finished_checking.load(Ordering::SeqCst) == true {
                println!("Finished");
                break;
            }

            link = links.borrow_mut().pop().unwrap();
        }

        {
            processing_units.fetch_add(1, Ordering::SeqCst);
            let processing_units = Arc::clone(&processing_units);
            let is_empty = Arc::clone(&is_empty);
            let client = client.clone();
            rt.spawn(future::lazy(|| process_url(link, processing_units, is_empty, client)));
        }
    }
}
