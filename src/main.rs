extern crate core;
extern crate hyper;
extern crate tokio;
extern crate futures;
extern crate hyper_tls;
extern crate regex;
extern crate url;
extern crate reqwest;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::io::Cursor;
use std::sync::mpsc;
use std::{mem, io, str, env};
use std::collections::BTreeMap;

use tokio::runtime;
use futures::{future, Future};
use hyper::StatusCode;
use futures::Stream;
use regex::Regex;
use reqwest::async::{Client, Decoder};
use core::fmt;

#[derive(Clone, Debug)]
enum ResponseStatus {
  Ok,
  Unreachable,
  Err(StatusCode),
  Processing
}

#[derive(Clone, Debug)]
struct Response {
  processed_link: String,
  status: ResponseStatus,
  extracted_links: Option<Vec<String>>,
}

fn process_url(url: String, processing_units: Arc<AtomicUsize>, client: Client, tx: mpsc::Sender<Response>) -> impl Future<Item=(), Error=()> {
  client
      .get(&url)
      .send()
      .then(move |res| {
        match res {
          Ok(mut res) => {
            let body = mem::replace(res.body_mut(), Decoder::empty());
            body.concat2()
                .map(move |body| {
                  let mut body = Cursor::new(body);
                  let mut writer: Vec<u8> = vec![];
                  let _ = io::copy(&mut body, &mut writer).map_err(|err| {
                    println!("stdout error: {}", err);
                  });

                  let body = str::from_utf8(&writer);

                  // @TODO: handle images or pdf...
                  if let Some(body) = body.ok() {
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
                    tx.send(Response {
                      processed_link: url,
                      extracted_links: Some(links),
                      status: ResponseStatus::Ok,
                    });
                  } else {
                    processing_units.fetch_sub(1, Ordering::SeqCst);
                    tx.send(Response {
                      processed_link: url,
                      status: ResponseStatus::Err(res.status()),
                      extracted_links: None,
                    });
                  }
                })
                .then(|_| {
                  future::ok(())
                })
          }
          Err(err) => {
            panic!("..., {:?}", err);
          }
        }
      })
}

#[derive(Debug)]
enum Error {
  ParseError,
  CrossOrigin,
}

impl From<url::ParseError> for Error {
  fn from(_e: url::ParseError) -> Error {
    Error::ParseError
  }
}

fn sanitize_link(scheme: &str, host: &str, link: &String) -> Result<String, Error> {
  if link.len() == 0 {
    return Err(Error::ParseError);
  }

  let is_relative = link.get(0..1).unwrap_or(" ") == "/";

  if is_relative {
    Ok(format!("{}://{}{}", scheme, host, link))
  }
  else {
    let parsed_url = url::Url::parse(link.as_str())?;
    let link_host = parsed_url.host_str();

    if let Some(l_host) = link_host {
      if l_host != host {
        Err(Error::CrossOrigin)
      }
      else {
        Ok(link.to_string())
      }

    } else {
      Err(Error::ParseError)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sanitize_link_test() {
    let scheme = "https";
    let host = "example.com";
    let link = String::from("/test.html");
    assert_eq!(sanitize_link(scheme, host, &link).unwrap(), String::from(format!("{}://{}{}", scheme, host, link)))
  }
}

fn main() {
  let url = match env::args().nth(1) {
    Some(url) => url,
    None => {
      println!("Usage: client <url>");
      return;
    }
  };

  let link = url::Url::parse(url.as_str()).unwrap();

  let host = link.host_str().unwrap();
  let scheme = link.scheme();

  let processing_units = Arc::new(AtomicUsize::new(0));

  let mut rt = runtime::Builder::new().core_threads(4).build().unwrap();

  let client = Client::new();

  let (tx, rx) = mpsc::channel();

  let mut sitemap = BTreeMap::new();

  {
    processing_units.fetch_add(1, Ordering::SeqCst);
    let processing_units = Arc::clone(&processing_units);
    let client = client.clone();
    let tx = mpsc::Sender::clone(&tx);
    let url = url.clone();
    sitemap.insert(url.clone(), Response {
      processed_link: url.clone(),
      extracted_links: None,
      status: ResponseStatus::Processing,
    });
    rt.spawn(future::lazy(|| process_url(url, processing_units, client, tx)));
  }

  for response in rx {
    sitemap.insert(response.processed_link.clone(), response.clone());
    if let Some(links) = response.extracted_links {
      for mut link in links {
        {
          match sanitize_link(scheme, host, &link) {
            Ok(sanitized_link) => {
              let contains = sitemap.contains_key(&sanitized_link);
              if !contains {
                sitemap.insert(sanitized_link.clone(), Response {
                  processed_link: sanitized_link.clone(),
                  extracted_links: None,
                  status: ResponseStatus::Processing,
                });
                let tx = mpsc::Sender::clone(&tx);
                processing_units.fetch_add(1, Ordering::SeqCst);
                let processing_units = Arc::clone(&processing_units);
                let client = client.clone();
                rt.spawn(future::lazy(|| process_url(sanitized_link, processing_units, client, tx)));
              }
            }
            Err(_error) => {}
          }
        }
      }
    }

    if processing_units.load(Ordering::SeqCst) == 0 {
      break;
    }
  }

  for (link, response) in &sitemap {
    println!("{}: \"{:?}\"", link, response.status);
  }

  println!("Finished checking website.");
}
