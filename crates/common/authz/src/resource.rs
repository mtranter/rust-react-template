use std::fmt::Debug;

use crate::Value;

/// A kind of row the policies speak of. Its actions are a closed enum, so no caller text reaches a
/// check; deriving `strum::VariantArray` and `strum::IntoStaticStr` (`serialize_all = "camelCase"`,
/// or `serialize = "items:add"` for a `:`-segment name) implements what `Action` needs.
pub trait Resource {
    const KIND: &'static str;
    /// Every name `attr` answers, so loading can check a policy's without a row.
    const ATTRS: &'static [&'static str];
    type Action: Action;
    /// The action that lets a principal see a row. A row they may not see is refused as not found,
    /// so its existence isn't revealed; without one, every refused row is.
    const VIEW: Option<Self::Action> = None;

    fn attr(&self, name: &str) -> Option<Value>;
}

pub trait Action: Copy + Eq + Debug + 'static {
    fn all() -> &'static [Self];
    fn name(self) -> &'static str;
}

impl<A> Action for A
where
    A: Copy + Eq + Debug + strum::VariantArray + Into<&'static str> + 'static,
{
    fn all() -> &'static [Self] {
        A::VARIANTS
    }

    fn name(self) -> &'static str {
        self.into()
    }
}
