use std::rc::Rc;
use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;

use crate::transpiler::{Bindable, Bootable, Castable, StringMap};

pub mod aws_lambda;
pub mod aws_lambda_alpine;
pub mod gloo;
pub mod k8s;
pub mod route53;

pub type ProviderMap = StringMap<Mutex<Box<dyn Provider + Send + Sync>>>;

pub static PROVIDERS: Lazy<ProviderMap> = Lazy::new(|| {
    let mut dns_providers = ProviderMap::new();
    dns_providers.insert(
        String::from("route53"),
        Mutex::new(Box::new(route53::DnsProvider::new())),
    );

    let mut map = ProviderMap::new();
    map.insert(
        String::from("aws-lambda"),
        Mutex::new(Box::new(aws_lambda::AwsLambdaProvider::new())),
    );
    map.insert(
        String::from("k8s"),
        Mutex::new(Box::new(k8s::KubernetesProvider::new(dns_providers))),
    );
    map
});

pub type Options = StringMap<String>;

pub trait Provider: Castable + Bindable + Bootable {
    fn name(&self) -> String;
    fn options(&self) -> Arc<Options>;
    fn set_options(&mut self, opts: Arc<Options>) -> Result<(), ProviderError>;
}

#[derive(Debug)]
pub enum ProviderError {
    TransformationError(String),
}

pub fn render_string_list(list: Rc<Vec<String>>) -> String {
    let mut out = String::from("[");
    for (i, l) in list.iter().enumerate() {
        out.push_str(&format!("\"{}\"", l));
        if i < list.len() - 1 {
            out.push_str(",");
        }
    }
    out.push_str("]");
    out
}
