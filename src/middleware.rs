use actix_session::SessionExt;
use actix_web::body::BoxBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::HttpResponse;
use std::future::{ready, Future, Ready};
use std::pin::Pin;

pub struct AuthGuard {
    pub context_path: String,
}

impl<S> Transform<S, ServiceRequest> for AuthGuard
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = actix_web::Error>
        + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = actix_web::Error;
    type Transform = AuthGuardMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthGuardMiddleware {
            service,
            context_path: self.context_path.clone(),
        }))
    }
}

pub struct AuthGuardMiddleware<S> {
    service: S,
    context_path: String,
}

impl<S> Service<ServiceRequest> for AuthGuardMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = actix_web::Error>
        + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(
        &self,
        ctx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(ctx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let path = req.path().to_string();

        let login_path = format!("{}/login", self.context_path);
        let static_path = format!("{}/static", self.context_path);

        // 放行登录页和静态资源
        if path == login_path || path.starts_with(&static_path) {
            let fut = self.service.call(req);
            return Box::pin(async move { fut.await });
        }

        // 检查 session
        let session = req.get_session();
        let is_logged_in = session
            .get::<String>("user")
            .unwrap_or(None)
            .is_some();

        if is_logged_in {
            let fut = self.service.call(req);
            Box::pin(async move { fut.await })
        } else {
            Box::pin(async move {
                let response = HttpResponse::Found()
                    .insert_header(("Location", login_path))
                    .finish();
                Ok(req.into_response(response).map_into_boxed_body())
            })
        }
    }
}
