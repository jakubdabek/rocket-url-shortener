#![deny(rust_2018_idioms)]

use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};

use rand::prelude::*;
use rocket::{
    http::{uri::Absolute, Status},
    response::{status::BadRequest, Redirect},
    serde::json::Json,
    tokio::sync::Mutex,
    State,
};

use self::stats::{CountMap, RequestCounter};

#[macro_use]
extern crate rocket;

mod stats;

// Async Mutex to not block the executor during requests,
// although locks should be held for short enough that it wouldn't matter.
// Could also consider `RwLock`, as `open` only needs read-only access.
type UriStore = Mutex<HashMap<u64, Absolute<'static>>>;

#[post("/shorten", data = "<url>")]
async fn shorten(
    url: String,
    uri_store: &State<UriStore>,
) -> Result<String, BadRequest<&'static str>> {
    // Could extend with additional error handling, e.g. only `https`, whitelist, etc.
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

    Ok(uri!("http://localhost:8000", open(id = key)).to_string())
}

#[get("/open/<id>")]
async fn open(id: u64, uri_store: &State<UriStore>) -> Result<Redirect, Status> {
    let uri = uri_store
        .lock()
        .await
        .get(&id)
        // redirects to a global error handler/catcher
        .ok_or(Status::NotFound)?
        .clone();
    Ok(Redirect::to(uri))
}

#[get("/counters")]
async fn counters(request_counter: &State<Arc<RequestCounter>>) -> Json<CountMap> {
    Json(request_counter.all().await)
}

#[launch]
fn rocket() -> _ {
    let request_counter = Arc::new(RequestCounter::default());
    rocket::build()
        .mount("/", routes![shorten, open, counters])
        .manage(UriStore::default())
        .manage(Arc::clone(&request_counter))
        .attach(request_counter)
}

#[cfg(test)]
mod test {
    use super::rocket;
    use rocket::http::uri::Absolute;
    use rocket::http::Status;
    use rocket::local::blocking::Client;

    #[test]
    fn open() {
        let client = Client::tracked(rocket()).expect("valid rocket instance");
        let response = client
            .post(uri!(super::shorten))
            .body("https://github.com/SergioBenitez/Rocket")
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let url = response.into_string().unwrap();
        let url = Absolute::parse(&url).unwrap();

        assert_eq!(url.authority().unwrap().host(), "localhost");
        assert_eq!(url.authority().unwrap().port(), Some(8000));

        let id = url
            .path()
            .as_str()
            .strip_prefix("/open/")
            .expect("wrong path prefix")
            .parse::<u64>()
            .expect("invalid id returned");

        let response = client.get(uri!(super::open(id = id))).dispatch();
        assert_eq!(response.status(), Status::SeeOther);

        let response = client
            .get(uri!(super::open(id = id.wrapping_add(1))))
            .dispatch();
        assert_eq!(response.status(), Status::NotFound);
    }

    #[test]
    fn invalid_url() {
        let client = Client::tracked(rocket()).expect("valid rocket instance");
        for url in ["/SergioBenitez/Rocket", "*", "?id=1"] {
            let response = client.post(uri!(super::shorten)).body(url).dispatch();
            assert_eq!(response.status(), Status::BadRequest);
            assert!(response.into_string().unwrap().contains("Invalid URL"));
        }
    }

    #[test]
    fn invalid_id() {
        let client = Client::tracked(rocket()).expect("valid rocket instance");
        let response = client.get(uri!(super::open(id = 123))).dispatch();
        assert_eq!(response.status(), Status::NotFound);

        let response = client.get(uri!("/open/abc")).dispatch();
        assert_eq!(response.status(), Status::NotFound);
    }
}
