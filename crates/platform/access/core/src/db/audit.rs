use crate::application::{
    AuditChange, AuditEntry, AuditFilter, AuditLog, AuditSortKey, ListQuery, Listed, Tx,
};
use access_api::events::v1::{
    AssignmentEvent, AssignmentTopic, GrantEvent, GrantTopic, RoleEvent, RoleTopic,
};
use app_db::{Generations, conn, push_list_clauses};
use app_error::{AppError, AppResult};
use app_events::TopicEnvelope;
use app_persistence::Handle;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

/// The queues feeding the first generation of the audit log: renaming one strands its copy,
/// so a new shape is a new generation (`_v2` queues and table).
pub const AUDIT_GRANTS: &str = "access.audit_grants";
pub const AUDIT_ROLES: &str = "access.audit_roles";
pub const AUDIT_ASSIGNMENTS: &str = "access.audit_assignments";

/// One row written per event, keyed by its id, so a redelivery writes nothing. Not an
/// `app_db::Project`: that keeps only each key's latest state, and a log keeps every event.
struct Written<'a> {
    id: Uuid,
    occurred_at: DateTime<Utc>,
    change: AuditChange,
    actor: Option<&'a str>,
    role: &'a str,
    subject: Option<&'a str>,
    grant: Option<(&'a str, &'a str, &'a str)>,
}

async fn write(tx: &mut Tx, entry: Written<'_>) -> AppResult<()> {
    let change: &'static str = entry.change.into();
    let (resource, action, scope) = match entry.grant {
        Some((resource, action, scope)) => (Some(resource), Some(action), Some(scope)),
        None => (None, None, None),
    };
    sqlx::query!(
        r#"
        INSERT INTO access.audit_entry
            (id, occurred_at, change, actor, role, subject, resource, action, scope)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (id) DO NOTHING
        "#,
        entry.id,
        entry.occurred_at,
        change,
        entry.actor,
        entry.role,
        entry.subject,
        resource,
        action,
        scope,
    )
    .execute(conn(tx))
    .await
    .map_err(AppError::internal)?;
    Ok(())
}

pub struct AuditRows;

#[async_trait]
impl Handle<GrantTopic> for AuditRows {
    async fn handle(&self, tx: &mut Tx, event: TopicEnvelope<GrantTopic>) -> AppResult<()> {
        let grant = &event.entity;
        let (change, actor) = match &event.event {
            GrantEvent::Granted => (AuditChange::AccessGranted, grant.granted_by.as_deref()),
            GrantEvent::Revoked(by) => (AuditChange::GrantRevoked, by.revoked_by.as_deref()),
        };
        write(
            tx,
            Written {
                id: event.id,
                occurred_at: event.time,
                change,
                actor,
                role: &grant.role,
                subject: None,
                grant: Some((&grant.resource, &grant.action, &grant.scope)),
            },
        )
        .await
    }
}

#[async_trait]
impl Handle<RoleTopic> for AuditRows {
    async fn handle(&self, tx: &mut Tx, event: TopicEnvelope<RoleTopic>) -> AppResult<()> {
        let role = &event.entity;
        let (change, actor) = match &event.event {
            RoleEvent::Created => (AuditChange::RoleCreated, role.created_by.as_deref()),
            RoleEvent::Deleted(by) => (AuditChange::RoleDeleted, by.deleted_by.as_deref()),
        };
        write(
            tx,
            Written {
                id: event.id,
                occurred_at: event.time,
                change,
                actor,
                role: &role.name,
                subject: None,
                grant: None,
            },
        )
        .await
    }
}

#[async_trait]
impl Handle<AssignmentTopic> for AuditRows {
    async fn handle(&self, tx: &mut Tx, event: TopicEnvelope<AssignmentTopic>) -> AppResult<()> {
        let assignment = &event.entity;
        let (change, actor) = match &event.event {
            AssignmentEvent::Assigned => {
                (AuditChange::RoleAssigned, assignment.assigned_by.as_deref())
            }
            AssignmentEvent::Unassigned(by) => {
                (AuditChange::RoleUnassigned, by.unassigned_by.as_deref())
            }
        };
        write(
            tx,
            Written {
                id: event.id,
                occurred_at: event.time,
                change,
                actor,
                role: &assignment.role,
                subject: Some(&assignment.subject),
                grant: None,
            },
        )
        .await
    }
}

#[derive(Clone, Copy)]
struct Tables {
    entries: &'static str,
}

/// Reads the audit log from the newest generation of its table that has caught up (ADR 0036).
pub struct PgAuditLog {
    pool: PgPool,
    generations: Generations<Tables>,
}

impl PgAuditLog {
    pub fn new(pool: PgPool) -> Self {
        let generations = Generations::new(pool.clone()).then(
            Tables {
                entries: "access.audit_entry",
            },
            &[AUDIT_GRANTS, AUDIT_ROLES, AUDIT_ASSIGNMENTS],
        );
        Self { pool, generations }
    }
}

fn sort_column(key: AuditSortKey) -> &'static str {
    match key {
        AuditSortKey::OccurredAt => "occurred_at",
    }
}

#[derive(sqlx::FromRow)]
struct AuditRow {
    id: Uuid,
    occurred_at: DateTime<Utc>,
    change: String,
    actor: Option<String>,
    role: String,
    subject: Option<String>,
    resource: Option<String>,
    action: Option<String>,
    scope: Option<String>,
}

impl TryFrom<AuditRow> for AuditEntry {
    type Error = AppError;

    fn try_from(r: AuditRow) -> AppResult<Self> {
        Ok(Self {
            id: r.id,
            occurred_at: r.occurred_at,
            change: r.change.parse().map_err(AppError::internal)?,
            actor: r.actor,
            role: r.role,
            subject: r.subject,
            resource: r.resource,
            action: r.action,
            scope: r.scope,
        })
    }
}

fn push_matching(sql: &mut QueryBuilder<Postgres>, tables: Tables, filter: &AuditFilter) {
    sql.push(format_args!(" FROM {} WHERE true", tables.entries));
    for (column, value) in [
        ("actor", &filter.actor),
        ("role", &filter.role),
        ("subject", &filter.subject),
    ] {
        if let Some(value) = value {
            sql.push(format_args!(" AND {column} = "))
                .push_bind(value.clone());
        }
    }
    if let Some(change) = filter.change {
        let change: &'static str = change.into();
        sql.push(" AND change = ").push_bind(change);
    }
    if let Some(since) = filter.since {
        sql.push(" AND occurred_at >= ").push_bind(since);
    }
    if let Some(until) = filter.until {
        sql.push(" AND occurred_at < ").push_bind(until);
    }
}

#[async_trait]
impl AuditLog for PgAuditLog {
    async fn find(
        &self,
        filter: &AuditFilter,
        query: &ListQuery<AuditSortKey>,
    ) -> AppResult<Listed<AuditEntry>> {
        let tables = self.generations.serving().await?;
        let mut sql = QueryBuilder::new(
            "SELECT id, occurred_at, change, actor, role, subject, resource, action, scope",
        );
        push_matching(&mut sql, tables, filter);
        push_list_clauses(&mut sql, query, sort_column, "seq DESC");
        let rows: Vec<AuditRow> = sql
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::internal)?;

        let total = match query.page {
            Some(_) => {
                let mut count = QueryBuilder::new("SELECT COUNT(*)");
                push_matching(&mut count, tables, filter);
                Some(
                    count
                        .build_query_scalar()
                        .fetch_one(&self.pool)
                        .await
                        .map_err(AppError::internal)?,
                )
            }
            None => None,
        };

        Ok(Listed {
            items: rows
                .into_iter()
                .map(AuditEntry::try_from)
                .collect::<AppResult<_>>()?,
            total,
        })
    }
}
