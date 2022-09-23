use std::borrow::Cow;
use std::collections::HashMap;

use rocket::fairing::{Fairing, Info, Kind};

use rocket::tokio::sync::Mutex;
use rocket::{Request, Response};

pub type CountMap = HashMap<Cow<'static, str>, usize>;

#[derive(Debug, Default)]
pub struct RequestCounter {
    count: Mutex<CountMap>,
}

impl RequestCounter {
    pub async fn add(&self, name: Cow<'static, str>) {
        *self.count.lock().await.entry(name).or_default() += 1;
    }

    pub async fn all(&self) -> CountMap {
        self.count.lock().await.clone()
    }
}

#[rocket::async_trait]
impl Fairing for RequestCounter {
    fn info(&self) -> Info {
        Info {
            name: "Request counter per route",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, _response: &mut Response<'r>) {
        let name = request
            .route()
            .expect("route")
            .name
            .as_ref()
            .expect("name")
            .clone();
        self.add(name).await;
    }
}
