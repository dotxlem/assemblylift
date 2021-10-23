pub mod package;

use std::collections::BTreeMap;

use macaroon::{Macaroon, Verifier, MacaroonKey};

pub type ActionList = Vec<Box<dyn Action>>;
pub type Capabilities = Macaroon;
pub type Shape = BTreeMap<String, String>;

/// Represents an Entity _instance_
pub trait Entity {
    fn id(&self) -> String;
    fn prototype(&self) -> String;
    fn caps(&self) -> Capabilities;
}
// TODO the Entity prototype must also be stored somewhere!
/// An Entity _prototype_ defining its shape and actions
pub struct EntityPrototype {
    pub name: String,
    actions: ActionList,
    shape: Shape,
}

impl EntityPrototype {
    pub fn new(name: &str, shape: Shape) -> Self {
        Self {
            name: name.to_string(),
            actions: Default::default(),
            shape,
        }
    }

    pub fn attach_action(&mut self, action: Box<dyn Action>) {
        self.actions.push(action)
    }
}

pub enum ActionRule {
    Deny,
    Allow,
}

pub enum ActionName {
    Load,
    Store,
}

/// An Entity _action_. Actions are capabilities!
pub trait Action {
    /// The name of this action
    fn name(&self) -> ActionName;
    /// Location in which the action is executed
    fn location(&self) -> String;
    /// Locations from which the action is allowed to originate (i.e. to receive from)
    fn origins(&self) -> Vec<String>;
    /// The rule applied to the action
    fn rule(&self) -> ActionRule;
}
