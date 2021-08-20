use log::{debug, error, warn};
use pointercrate_core::{
    error::{CoreError, PointercrateError},
    pool::{audit_connection, PointercratePool},
};
use pointercrate_user::{error::UserError, AuthenticatedUser, User};
use rocket::{
    http::{Method, Status},
    request::{FromRequest, Outcome},
    Request, State,
};
use sqlx::{PgConnection, Postgres, Transaction};
use std::fmt::Debug;

pub struct Auth<const IsToken: bool>(pub AuthenticatedUser, pub Transaction<'static, Postgres>);

pub type BasicAuth = Auth<false>;
pub type TokenAuth = Auth<true>;

macro_rules! try_outcome {
    ($outcome:expr) => {
        match $outcome {
            Ok(success) => success,
            Err(error) => return Outcome::Failure((Status::from_code(error.status_code()).unwrap(), error.into())),
        }
    };
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Auth<true> {
    type Error = UserError;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        // No auth header set, forward to the request handler that doesnt require authorization
        if request.headers().get_one("Authorization").is_none() && request.cookies().get("access_token").is_none() {
            return Outcome::Forward(())
        }

        let pool = request.guard::<&State<PointercratePool>>().await;

        let mut connection = match pool {
            Outcome::Success(pool) => try_outcome!(pool.transaction().await),
            _ => {
                error!("Could not retrieve database pool from shared state. Did you correctly configure rocket state?");

                return Outcome::Failure((Status::InternalServerError, CoreError::InternalServerError.into()))
            },
        };

        for authorization in request.headers().get("Authorization") {
            if let &["Bearer", token] = &authorization.split(' ').collect::<Vec<_>>()[..] {
                let user =
                    try_outcome!(AuthenticatedUser::token_auth(token, None, &pointercrate_core::config::secret(), &mut connection).await);

                try_outcome!(audit_connection(&mut connection, user.inner().id).await);

                return Outcome::Success(Auth(user, connection))
            }
        }

        // no matching auth header, lets try the cookie
        if let Some(access_token) = request.cookies().get("access_token") {
            let access_token = access_token.value();

            if request.method() == Method::Get {
                debug!("GET request, the cookie is enough");

                let user = try_outcome!(
                    AuthenticatedUser::token_auth(access_token, None, &pointercrate_core::config::secret(), &mut connection).await
                );

                try_outcome!(audit_connection(&mut connection, user.inner().id).await);

                return Outcome::Success(Auth(user, connection))
            }

            debug!("Non-GET request, testing X-CSRF-TOKEN header");
            // if we're doing cookie based authorization, there needs to be a X-CSRF-TOKEN
            // header set, unless we're in GET requests, in which case everything is fine
            // :tm:

            if let Some(csrf_token) = request.headers().get_one("X-CSRF-TOKEN") {
                let user = try_outcome!(
                    AuthenticatedUser::token_auth(
                        access_token,
                        Some(csrf_token),
                        &pointercrate_core::config::secret(),
                        &mut connection
                    )
                    .await
                );

                try_outcome!(audit_connection(&mut connection, user.inner().id).await);

                return Outcome::Success(Auth(user, connection))
            } else {
                warn!("Cookie based authentication was used, but no CSRF-token was provided. This might be a CSRF attack!");
            }
        }

        Outcome::Failure((Status::Unauthorized, CoreError::Unauthorized.into()))
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Auth<false> {
    type Error = UserError;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        // No auth header set, forward to the request handler that doesnt require authorization
        if request.headers().get_one("Authorization").is_none() {
            return Outcome::Forward(())
        }

        let pool = request.guard::<&State<PointercratePool>>().await;

        let mut connection = match pool {
            Outcome::Success(pool) => try_outcome!(pool.transaction().await),
            _ => {
                error!("Could not retrieve database pool from shared state. Did you correctly configure rocket state?");

                return Outcome::Failure((Status::InternalServerError, CoreError::InternalServerError.into()))
            },
        };

        for authorization in request.headers().get("Authorization") {
            if let &["Basic", basic_auth] = &authorization.split(' ').collect::<Vec<_>>()[..] {
                let decoded = try_outcome!(base64::decode(basic_auth)
                    .map_err(|_| ())
                    .and_then(|bytes| String::from_utf8(bytes).map_err(|_| ()))
                    .map_err(|_| {
                        warn!("Malformed 'Authorization' header");

                        CoreError::InvalidHeaderValue { header: "Authorization" }
                    }));

                if let [username, password] = &decoded.splitn(2, ':').collect::<Vec<_>>()[..] {
                    let user = try_outcome!(AuthenticatedUser::basic_auth(*username, *password, &mut connection).await);

                    try_outcome!(audit_connection(&mut connection, user.inner().id).await);

                    return Outcome::Success(Auth(user, connection))
                }
            }
        }

        Outcome::Failure((Status::Unauthorized, CoreError::Unauthorized.into()))
    }
}