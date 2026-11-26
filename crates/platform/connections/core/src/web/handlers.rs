use super::models::{
    ConnectionResponse, ConnectionSort, SetSecretRequest, UpdateConnectionRequest,
};
use crate::application::{ConnectionService, Reported, Versioned};
use crate::domain::{Connection, UpdateConnection};
use app_error::AppResult;
use app_web::{ApiResult, Caller, ListResult, WithETag, if_match_version, paged, sorts};
use poem::web::Data;
use poem_openapi::types::MaybeUndefined;
use poem_openapi::{
    OpenApi, Tags,
    param::{Header, Path, Query},
    payload::Json,
};

type ConnectionResult = WithETag<ConnectionResponse>;

#[derive(Tags)]
enum ApiTag {
    Connections,
}

fn tagged(reported: Reported) -> ConnectionResult {
    let version = reported.connection.version;
    ConnectionResult::new(reported.into(), version)
}

async fn reported(
    connections: &ConnectionService,
    connection: AppResult<Versioned<Connection>>,
) -> ApiResult<ConnectionResult> {
    Ok(tagged(connections.report(connection?).await?))
}

/// Unit struct so `dump-spec` can build the spec without a database.
#[derive(Default)]
pub struct ConnectionsEndpoints;

#[OpenApi(prefix_path = "/connections", tag = "ApiTag::Connections")]
impl ConnectionsEndpoints {
    /// List the Connections to other systems the app's modules declare, by key unless `sort`
    /// says otherwise, with each one's URL, fields and which secrets are set (never their
    /// values). Admins only: anyone else is refused `403`.
    #[oai(path = "/", method = "get", operation_id = "list_connections")]
    async fn list(
        &self,
        connections: Data<&ConnectionService>,
        caller: Caller,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
        sort: Query<Option<Vec<ConnectionSort>>>,
    ) -> ApiResult<Json<ListResult<ConnectionResponse>>> {
        let query = paged(limit.0, offset.0).sorted(sorts(sort.0));
        let page = query.page;
        let listed = connections.0.list(&caller, query).await?;
        let items = connections.0.reported(listed.items).await?;
        let listed = app_persistence::Listed {
            items,
            total: listed.total,
        };
        Ok(Json(ListResult::new(listed, page)))
    }

    /// Fetch a Connection by its key: its URL, fields, and for each secret whether it is set,
    /// when and by whom (never its value). The response carries its version in the `ETag`
    /// header (and body), which a later change must send back in `If-Match`. Admins only:
    /// anyone else is refused `403`; a key no module declares is `404`.
    #[oai(path = "/:key", method = "get", operation_id = "get_connection")]
    async fn get(
        &self,
        connections: Data<&ConnectionService>,
        caller: Caller,
        key: Path<String>,
    ) -> ApiResult<ConnectionResult> {
        reported(connections.0, connections.0.get(&caller, &key.0).await).await
    }

    /// Change a Connection's URL or its declared non-secret fields. Requires `If-Match` with
    /// the version last read; a stale one is refused `412` and changes nothing. Admins only:
    /// anyone else is refused `403`.
    #[oai(path = "/:key", method = "patch", operation_id = "update_connection")]
    async fn update(
        &self,
        connections: Data<&ConnectionService>,
        caller: Caller,
        key: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
        body: Json<UpdateConnectionRequest>,
    ) -> ApiResult<ConnectionResult> {
        let expected = if_match_version(if_match.0)?;
        let url = match body.0.url {
            MaybeUndefined::Undefined => None,
            MaybeUndefined::Null => Some(None),
            MaybeUndefined::Value(url) => Some(Some(url)),
        };
        let cmd = UpdateConnection {
            url,
            fields: body.0.fields.unwrap_or_default(),
        };
        let connection = connections.0.update(&caller, &key.0, cmd, expected).await;
        reported(connections.0, connection).await
    }

    /// Set one of a Connection's secrets, sealed before it is stored, replacing any it had
    /// (which is how a key is rotated). Requires `If-Match` with the Connection's version;
    /// stale is `412`. Answers `501` when no master key is configured. Admins only: anyone
    /// else is refused `403`.
    #[oai(
        path = "/:key/secrets/:name",
        method = "put",
        operation_id = "set_connection_secret"
    )]
    async fn set_secret(
        &self,
        connections: Data<&ConnectionService>,
        caller: Caller,
        key: Path<String>,
        name: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
        body: Json<SetSecretRequest>,
    ) -> ApiResult<ConnectionResult> {
        let expected = if_match_version(if_match.0)?;
        let connection = connections
            .0
            .set_secret(&caller, &key.0, &name.0, &body.0.value, expected)
            .await;
        reported(connections.0, connection).await
    }

    /// Clear one of a Connection's secrets; clearing one that isn't set changes nothing.
    /// Requires `If-Match` with the Connection's version; stale is `412`. Admins only: anyone
    /// else is refused `403`.
    #[oai(
        path = "/:key/secrets/:name",
        method = "delete",
        operation_id = "clear_connection_secret"
    )]
    async fn clear_secret(
        &self,
        connections: Data<&ConnectionService>,
        caller: Caller,
        key: Path<String>,
        name: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
    ) -> ApiResult<ConnectionResult> {
        let expected = if_match_version(if_match.0)?;
        let connection = connections
            .0
            .clear_secret(&caller, &key.0, &name.0, expected)
            .await;
        reported(connections.0, connection).await
    }

    /// Test a Connection: send the health check its module declares, and record and answer
    /// what it found, `status` `ok` or `failing` with `lastError` saying why in plain language.
    /// Changes neither the Connection nor its version, so it takes no `If-Match`. Refused `501`
    /// while the Connection lacks its URL or a credential, and `400` when it declares no health
    /// check. Admins only: anyone else is refused `403`.
    #[oai(path = "/:key/test", method = "post", operation_id = "test_connection")]
    async fn test(
        &self,
        connections: Data<&ConnectionService>,
        caller: Caller,
        key: Path<String>,
    ) -> ApiResult<ConnectionResult> {
        Ok(tagged(connections.0.test(&caller, &key.0).await?))
    }
}
