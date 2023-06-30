use axum::{
    async_trait,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
#[allow(unused_imports)]
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{net::SocketAddr, sync::Arc};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "error_handling_and_dependency_injection=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let user_repo = Arc::new(ExampleUserRepo) as DynUserRepo;
    let app = app(user_repo);

    let addr = SocketAddr::from(([127, 0, 0, 1], 4444));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

fn app(user_repo: DynUserRepo) -> Router {
    Router::new()
        .route("/users/:user_id", get(users_show))
        .route("/users", post(users_create))
        .with_state(user_repo)
}

async fn users_show(
    Path(user_id): Path<Uuid>,
    State(repo): State<DynUserRepo>,
) -> Result<Json<User>, AppError> {
    let user = repo.find(user_id).await?;
    Ok(user.into())
}

async fn users_create(
    State(repo): State<DynUserRepo>,
    Json(params): Json<CreateUser>,
) -> Result<Json<User>, AppError> {
    let user = repo.create(params).await?;
    Ok(user.into())
}

#[derive(Debug)]
enum AppError {
    UserRepo(UserRepoError),
}

impl From<UserRepoError> for AppError {
    fn from(err: UserRepoError) -> Self {
        AppError::UserRepo(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::debug!("AppError error into response {:?}", self);
        let (status, error_message) = match self {
            AppError::UserRepo(UserRepoError::NotFound) => {
                (StatusCode::NOT_FOUND, "user not found")
            }
            AppError::UserRepo(UserRepoError::InvalidUsername) => {
                (StatusCode::BAD_REQUEST, "invalid username")
            }
        };

        let body = Json(json!({ "error": error_message }));

        (status, body).into_response()
    }
}
struct ExampleUserRepo;

#[async_trait]
impl UserRepo for ExampleUserRepo {
    async fn find(&self, id: Uuid) -> Result<User, UserRepoError> {
        tracing::debug!("finding user {:?}", id);
        let s = id.to_string();
        if s.chars().next().map_or(false, |c| c != 'a') {
            Ok(User {
                id,
                username: "example".to_string(),
            })
        } else {
            Err(UserRepoError::NotFound)
        }
    }

    async fn create(&self, _params: CreateUser) -> Result<User, UserRepoError> {
        let uuid = Uuid::new_v4();
        Ok(User {
            id: uuid,
            username: "new example".to_string(),
        })
    }
}

type DynUserRepo = Arc<dyn UserRepo + Send + Sync>;

#[async_trait]
trait UserRepo {
    async fn find(&self, id: Uuid) -> Result<User, UserRepoError>;

    async fn create(&self, params: CreateUser) -> Result<User, UserRepoError>;
}

#[derive(Debug, Serialize)]
struct User {
    id: Uuid,
    username: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CreateUser {
    username: String,
}

#[derive(Debug)]
enum UserRepoError {
    #[allow(dead_code)]
    NotFound,

    #[allow(dead_code)]
    InvalidUsername,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{self, Request, StatusCode},
    };
    use serde_json::Value;
    // use serde_json::json;
    // use std::net::SocketAddr;
    // use tokio::net::TcpListener;
    // use tower::Service; // for `call`
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_users_show_found() {
        let user_repo = Arc::new(ExampleUserRepo) as DynUserRepo;
        let app = app(user_repo);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users/bd8197e0-8a30-4e7c-9c93-972fd13ed4c8")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_users_show_not_found() {
        let user_repo = Arc::new(ExampleUserRepo) as DynUserRepo;
        let app = app(user_repo);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users/ad8197e0-8a30-4e7c-9c93-972fd13ed4c8")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_users_create() {
        let user_repo = Arc::new(ExampleUserRepo) as DynUserRepo;
        let app = app(user_repo);

        let response = app
            .oneshot(
                Request::builder()
                    .method(http::Method::POST)
                    .uri("/users")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"username":"example"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        let id = body.get("id").unwrap().as_str().unwrap();

        assert_eq!(id.len(), 36);
        let name = body
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or("no name");
        assert_eq!(name, "new example");
    }
}
