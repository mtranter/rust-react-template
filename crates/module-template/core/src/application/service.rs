use super::events;
use super::{
    EventPublisher, Expected, ListQuery, Listed, PageRequest, Transactions, Version, Versioned,
    WidgetMatch, WidgetRepository, WidgetSearch, WidgetSortKey,
};
use crate::domain::{CreateWidget, RenameWidget, Widget, WidgetAction, WidgetId};
use app_authz::{Authorizer, Plan, Principal};
use app_domain::Clock;
use app_error::{AppError, AppResult};
use std::sync::Arc;

#[derive(Clone)]
pub struct WidgetService {
    repo: Arc<dyn WidgetRepository>,
    search: Arc<dyn WidgetSearch>,
    publisher: Arc<dyn EventPublisher>,
    transactions: Arc<dyn Transactions>,
    clock: Arc<dyn Clock>,
    authorizer: Arc<Authorizer>,
}

impl WidgetService {
    pub fn new(
        repo: Arc<dyn WidgetRepository>,
        search: Arc<dyn WidgetSearch>,
        publisher: Arc<dyn EventPublisher>,
        transactions: Arc<dyn Transactions>,
        clock: Arc<dyn Clock>,
        authorizer: Arc<Authorizer>,
    ) -> Self {
        Self {
            repo,
            search,
            publisher,
            transactions,
            clock,
            authorizer,
        }
    }

    /// Asked first in every use case, before anything is loaded or begun. A
    /// `Where` plan (a scope) is checked against the widget once it is loaded
    /// ([`load_for`](Self::load_for)).
    fn authorize(&self, principal: &Principal, action: WidgetAction) -> AppResult<Plan> {
        self.authorizer.authorize::<Widget>(principal, action)
    }

    pub async fn create(
        &self,
        principal: &Principal,
        cmd: CreateWidget,
    ) -> AppResult<Versioned<Widget>> {
        self.authorize(principal, WidgetAction::Create)?;
        let (widget, event) = Widget::create(WidgetId::new(), cmd, self.clock.now())?;
        let mut tx = self.transactions.begin().await?;
        let version = self.repo.save(&mut tx, &widget, Expected::New).await?;
        self.publisher
            .publish(&mut tx, events::pending(&widget, version, event))
            .await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: widget,
            version,
        })
    }

    /// `expected` is the version the caller last read (from `If-Match`).
    pub async fn rename(
        &self,
        principal: &Principal,
        cmd: RenameWidget,
        expected: Version,
    ) -> AppResult<Versioned<Widget>> {
        let plan = self.authorize(principal, WidgetAction::Rename)?;
        // Load first so a missing widget is a not-found rather than a stale-version
        // conflict; the version check itself is atomic in the repository's update.
        let mut widget = self.load_for(principal, &plan, cmd.widget_id).await?.entity;
        let event = widget.rename(cmd)?;
        let mut tx = self.transactions.begin().await?;
        let version = self
            .repo
            .save(&mut tx, &widget, Expected::Version(expected))
            .await?;
        self.publisher
            .publish(&mut tx, events::pending(&widget, version, event))
            .await?;
        tx.commit().await?;
        Ok(Versioned {
            entity: widget,
            version,
        })
    }

    pub async fn get(&self, principal: &Principal, id: WidgetId) -> AppResult<Versioned<Widget>> {
        let plan = self.authorize(principal, WidgetAction::View)?;
        self.load_for(principal, &plan, id).await
    }

    /// The widget, if `plan` allows it: checked before any domain action, so a
    /// widget the principal may not see is not found, and one they may see but
    /// not change is forbidden.
    async fn load_for(
        &self,
        principal: &Principal,
        plan: &Plan,
        id: WidgetId,
    ) -> AppResult<Versioned<Widget>> {
        let widget = self
            .repo
            .get(id)
            .await?
            .ok_or_else(|| AppError::not_found("widget"))?;
        self.authorizer.check(principal, plan, &widget.entity)?;
        Ok(widget)
    }

    pub async fn list(
        &self,
        principal: &Principal,
        query: ListQuery<WidgetSortKey>,
    ) -> AppResult<Listed<Versioned<Widget>>> {
        let rows = self
            .authorizer
            .rows::<Widget>(principal, WidgetAction::List)?;
        self.repo.list(&query, &rows).await
    }

    /// From the search copy, so a write shows here once the worker has
    /// delivered its event, unlike [`get`](Self::get) and [`list`](Self::list).
    pub async fn search(
        &self,
        principal: &Principal,
        name: &str,
        page: PageRequest,
    ) -> AppResult<Listed<WidgetMatch>> {
        // The copy has no columns for a scope to filter by, so a scoped search
        // is refused rather than shown rows the caller may not see.
        if self.authorize(principal, WidgetAction::Search)? != Plan::Allowed {
            return Err(AppError::Forbidden);
        }
        self.search.search(name, page).await
    }
}
