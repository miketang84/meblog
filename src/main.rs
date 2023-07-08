#![allow(unused)]
use askama::Template;
use axum::{
    extract::{Query, RawQuery, State},
    http::{response, uri::Uri, Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use serde::Serializer;
use std::net::SocketAddr;
use std::sync::Arc;
// use tower_http::services::{ServeDir, ServeFile};
use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

mod article;
mod comment;
mod index;
mod subspace;
mod user;

pub struct AppStateInner {
    // hclient: reqwest::Client,
    rclient: redis::Client,
}

pub type AppState = Arc<AppStateInner>;

pub type LoggedUserId = Option<String>;

pub const APPPROFESSION: &str = "it";
pub const APPID: &str = "discux";

// The customized middleware
async fn top_middleware<B>(
    State(app_state): State<AppState>,
    // you can add more extractors here but the last
    // extractor must implement `FromRequest` which
    // `Request` does
    cookie_jar: CookieJar,
    mut req: Request<B>,
    next: Next<B>,
) -> Response {
    // do something with `request`...
    let cookie_key = format!("{}_sid", &APPID);
    if let Some(session_id) = cookie_jar.get(&cookie_key) {
        let mut redis_conn = app_state.rclient.get_async_connection().await.unwrap();
        // check this session id with redis
        let key = format!("{}_session:{}", &APPID, session_id);

        let result: Result<String, redis::RedisError> = redis_conn.get(&key).await;
        if let Ok(user_id) = result {
            // insert this user_id to request extension
            req.extensions_mut().insert(Some(user_id));
        } else {
            // no this session, do nothing
        }
    } else {
        // no cookie, do nothing
    }

    let response = next.run(req).await;

    // do something with `response`...

    response
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    // let http_client = reqwest::Client::new();
    let redis_client = redis::Client::open("redis://127.0.0.1/").unwrap();

    let app_state: AppState = Arc::new(AppStateInner {
        // hclient: http_client,
        rclient: redis_client,
    });

    let app = Router::new()
        .route("/", get(index::view_index))
        .route("/subspace", get(subspace::view_subspace))
        .route(
            "/subspace/create",
            get(subspace::view_subspace_create).post(subspace::post_subspace_create),
        )
        .route(
            "/subspace/delete",
            get(subspace::view_subspace_delete).post(subspace::post_subspace_delete),
        )
        .route("/article", get(article::view_article))
        .route(
            "/article/create",
            get(article::view_article_create).post(article::post_article_create),
        )
        .route(
            "/article/edit",
            get(article::view_article_edit).post(article::post_article_edit),
        )
        .route(
            "/article/delete",
            get(article::view_article_delete).post(article::post_article_delete),
        )
        .route(
            "/comment/create",
            get(comment::view_comment_create).post(comment::post_comment_create),
        )
        .route(
            "/comment/delete",
            get(comment::view_comment_delete).post(comment::post_comment_delete),
        )
        .route("/user/account", get(user::view_account))
        .route("/user/signout", get(user::signout))
        .route("/user/login", post(user::view_login))
        .route("/error/info", get(view_error_info))
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            top_middleware,
        ))
        .with_state(app_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3333));
    println!("reverse proxy listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

/// helper function
// call it like:  make_get::<GutpPost>(...)
// or: let avec: Vec<GutpPost> = make_get(...)
pub async fn make_get<T: DeserializeOwned, U: Serialize + ?Sized>(
    path: &str,
    query_param: &U,
) -> anyhow::Result<Vec<T>> {
    let host = "http://127.0.0.1:3000";
    let url = format!("{}{}", host, path);

    let client = reqwest::Client::new();
    let list: Vec<T> = client
        .get(&url)
        .query(query_param)
        .header("User-Agent", "gutp-discux")
        .send()
        .await?
        .json() // convert the response to coresponding rust type
        .await?;

    // println!("in make_get res: {:?}", list);
    Ok(list)
}

pub async fn make_post<T: DeserializeOwned, U: Serialize + ?Sized>(
    path: &str,
    form_param: &U,
) -> anyhow::Result<Vec<T>> {
    let host = "http://127.0.0.1:3000";
    let url = format!("{}{}", host, path);

    let client = reqwest::Client::new();
    let list: Vec<T> = client
        .post(&url)
        .form(form_param)
        .header("User-Agent", "gutp-discux")
        .send()
        .await?
        .json() // convert the response to coresponding rust type
        .await?;

    // println!("in make_get res: {:?}", list);
    Ok(list)
}

/// Define the template handler
pub struct HtmlTemplate<T>(T);

impl<T> IntoResponse for HtmlTemplate<T>
where
    T: Template,
{
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to render template. Error: {}", err),
            )
                .into_response(),
        }
    }
}

#[derive(Template, Deserialize)]
#[template(path = "action_error.html")]
struct ErrorInfoTemplate {
    action: String,
    err_info: String,
}

async fn view_error_info(Query(params): Query<ErrorInfoTemplate>) -> impl IntoResponse {
    // render the page
    HtmlTemplate(ErrorInfoTemplate {
        action: params.action,
        err_info: params.err_info,
    })
}

pub fn redirect_to_error_page(action: &str, err_info: &str) -> Redirect {
    let redirect_uri = format!("/error/info?action={}&err_info={}", action, err_info);
    Redirect::to(&redirect_uri)
}
