extern crate core;
extern crate hyper;
//extern crate futures;
//extern crate hyper_tls;
//extern crate tokio;

use std::rc::Rc;
use std::cell::RefCell;
use std::env;
use std::sync::{Arc, Mutex, Condvar};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::atomic::Ordering;
use std::thread;

use core::time;

use hyper::Uri;
//use futures::{future, Future};
//use hyper_tls::HttpsConnector;
//use hyper::{Client, StatusCode, Response};
//use hyper::rt::{self, Stream};
//use std::io;
//use std::io::Write;
//use hyper::service::service_fn;
//use hyper::Body;
//use hyper::client::HttpConnector;
//use std::borrow::BorrowMut;

//fn fetch_uri(url: Uri, client: &Client<HttpsConnector<HttpConnector>>) -> Box<Future<Item = (), Error = ()> + Send + 'static> {
//    Box::new(tokio::run(future::lazy(|| {
//        client
//            .get(url)
//            .map(|res| {
//                if res.status() == StatusCode::MOVED_PERMANENTLY {
//                    println!("{}", res.headers().get("Location").unwrap().to_str().unwrap());
//                } else {
//                    println!("{}", res.status());
//                }
//            })
//            .map(|_| {
//                println!("\n\nFinished checking website.");
//            })
//            .map_err(|e| eprintln!("request error: {}", e))
//    })))
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

    links.borrow_mut().push(url);

    let mutex = Arc::new(Mutex::new(false));
    let is_empty = Arc::new(Condvar::new());
    let has_finished_checking = Arc::new(AtomicBool::new(false));
    let processing_units = Arc::new(AtomicUsize::new(0));

    loop {
        println!("Looping");
        let link;

        {
            let mut mutex_guard = mutex.lock().unwrap();

            if processing_units.load(Ordering::SeqCst) == 0 && links.borrow().is_empty() {
                has_finished_checking.store(true, Ordering::SeqCst);
            }

            if has_finished_checking.load(Ordering::SeqCst) {
                println!("Finished");
                break;
            }

            if links.borrow().is_empty() {
                println!("Empty");
                mutex_guard = is_empty.wait(mutex_guard).unwrap();
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

        let process_url = |url| {
            let processing_units = Arc::clone(&processing_units);
            println!("Processed {}", url);
            thread::sleep(time::Duration::from_millis(10));
            processing_units.fetch_sub(1, Ordering::SeqCst);
            is_empty.notify_one();
        };


        {
            let processing_units = Arc::clone(&processing_units);
            processing_units.fetch_add(1, Ordering::SeqCst);
            process_url(link);
        }
    }

//    tokio::run(future::lazy(|| {
    // 4 is number of blocking DNS threads
//    let https = HttpsConnector::new(4).unwrap();
//    let client = Client::builder()
//        .build::<_, hyper::Body>(https);
//
//
//    let new_service = move |url| {
//        let client = client.clone();
//        service_fn(move |res| {
//            fetch_uri(url, &client);
//        })
//    };

//        while !has_finished_checking {
//    new_service(url);
//        }
//    }));
}


//}
