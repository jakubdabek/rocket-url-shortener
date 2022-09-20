#![deny(rust_2018_idioms)]

use std::collections::{hash_map::Entry, HashMap};

use rand::prelude::*;
use rocket::{
    http::uri::Absolute,
    response::{
        status::{BadRequest, NotFound},
        Redirect,
    },
    tokio::sync::Mutex,
    State,
};

#[macro_use]
extern crate rocket;

// Async Mutex to not block the executor during requests,
// although locks should be held for short enough that it wouldn't matter.
type UriStore = Mutex<HashMap<u64, Absolute<'static>>>;

#[post("/shorten", data = "<url>")]
async fn shorten(
    url: String,
    uri_store: &State<UriStore>,
) -> Result<String, BadRequest<&'static str>> {
    let url = Absolute::parse_owned(url).map_err(|_| BadRequest(Some("Invalid URL")))?;
    let url = url.into_normalized();
    let key = {
        let mut uri_store = uri_store.lock().await;
        let rng = &mut thread_rng();
        loop {
            let key = rng.gen();
            match uri_store.entry(key) {
                Entry::Occupied(_) => continue,
                Entry::Vacant(v) => {
                    v.insert(url);
                    break key;
                }
            }
        }
    };

    Ok(format!("http://localhost:8000{}", uri!(open(id = key))))
}

#[get("/open/<id>")]
async fn open(id: u64, uri_store: &State<UriStore>) -> Result<Redirect, NotFound<&'static str>> {
    let uri = uri_store
        .lock()
        .await
        .get(&id)
        .ok_or(NotFound("Given link doesn't exist"))?
        .clone();
    Ok(Redirect::to(uri))
}

#[launch]
fn rocket() -> _ {
    rocket::build()
        .mount("/", routes![shorten, open])
        .manage(UriStore::default())
}
