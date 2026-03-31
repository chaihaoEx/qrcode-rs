//! 认证守卫中间件
//!
//! 实现 Actix-web 的 `Transform` / `Service` 接口，对所有请求进行会话认证检查。
//! 未认证的请求将被重定向到登录页面。
//!
//! 放行规则（无需认证即可访问）：
//! - `/login` — 登录页面
//! - `/static/*` — 静态资源文件
//! - `/extract/*` — 公开的二维码提取页面

use actix_session::SessionExt;
use actix_web::body::BoxBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::HttpResponse;
use std::future::{ready, Future, Ready};
use std::pin::Pin;

/// 认证守卫工厂，用于在 Actix-web 中间件链中注册。
///
/// 通过 `App::wrap()` 注册后，会为每个请求创建 `AuthGuardMiddleware` 实例。
pub struct AuthGuard {
    /// 虚拟目录前缀（如 `"/qrcode"`），用于构造放行路径
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

/// 认证守卫中间件实例，每个请求经过此中间件进行认证检查。
pub struct AuthGuardMiddleware<S> {
    /// 被包装的下游服务
    service: S,
    /// 虚拟目录前缀
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
        let extract_path = format!("{}/extract/", self.context_path);

        // ---- 公开路径放行 ----
        // 登录页、静态资源和公开提取页无需认证
        if path == login_path || path.starts_with(&static_path) || path.starts_with(&extract_path) {
            log::debug!("AuthGuard: public path, pass through: {path}");
            let fut = self.service.call(req);
            return Box::pin(async move { fut.await });
        }

        // ---- 检查会话认证 ----
        let session = req.get_session();
        let username = session.get::<String>("user").unwrap_or(None);

        if let Some(ref user) = username {
            // 已认证，放行请求
            log::debug!("AuthGuard: authenticated user={user}, path={path}");
            let fut = self.service.call(req);
            Box::pin(async move { fut.await })
        } else {
            // 未认证，重定向到登录页
            log::info!("AuthGuard: unauthenticated access to {path}, redirecting to login");
            Box::pin(async move {
                let response = HttpResponse::Found()
                    .insert_header(("Location", login_path))
                    .finish();
                Ok(req.into_response(response).map_into_boxed_body())
            })
        }
    }
}
