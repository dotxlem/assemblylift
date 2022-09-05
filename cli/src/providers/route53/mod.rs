use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use handlebars::Handlebars;
use itertools::Itertools;
use jsonpath_lib::Selector;
use serde::Serialize;

use crate::providers::{Options, Provider, ProviderError, AWS_LAMBDA_PROVIDER_NAME, KUBERNETES_PROVIDER_NAME, ROUTE53_PROVIDER_NAME};
use crate::tools::kubectl::KubeCtl;
use crate::transpiler::context::Context;
use crate::transpiler::{Artifact, Bindable, Bootable, CastError, Castable, ContentType, Template};

pub struct DnsProvider {
    /// access_key_id, secret_key, aws_region
    options: Arc<Options>,
}

impl DnsProvider {
    pub fn new() -> Self {
        Self {
            options: Arc::new(Options::new()),
        }
    }

    fn gloo_proxy_ip(&self) -> String {
        let mut labels = HashMap::new();
        labels.insert("gloo".to_string(), "gateway-proxy".to_string());
        let kubectl = KubeCtl::default();
        let gateways = kubectl
            .get_in_namespace("services", "gloo-system", Some(labels))
            .unwrap();
        let mut selector = Selector::new();
        selector
            .str_path("$.items[0].status.loadBalancer.ingress[0].ip")
            .unwrap()
            .value(&gateways)
            .select_as_str()
            .unwrap()
    }
}

impl Castable for DnsProvider {
    fn cast(&self, ctx: Rc<Context>, _selector: Option<&str>) -> Result<Vec<Artifact>, CastError> {
        let project_name = ctx.project.name.clone();
        let zones = ctx
            .domains
            .iter()
            .filter(|&d| d.provider.name == self.name())
            .map(|d| {
                let records = ctx
                    .services
                    .iter()
                    .filter(|&s| s.domain_name == Some(d.dns_name.clone()))
                    .map(|s| {
                        let target = s.provider.name.clone();
                        Record {
                            name: s.name.clone(),
                            target: match target {
                                _ if { target.eq_ignore_ascii_case(KUBERNETES_PROVIDER_NAME) } => {
                                    self.gloo_proxy_ip()
                                }
                                _ => "".to_string(),
                            },
                            is_apigw_target: match target {
                                _ if { target.eq_ignore_ascii_case(AWS_LAMBDA_PROVIDER_NAME) } => true,
                                _ => false,
                            },
                        }
                    })
                    .collect_vec();

                Zone {
                    name: d.dns_name.clone(),
                    name_snaked: d.dns_name.replace(".", "_"),
                    records,
                    options: d.provider.options.clone(),
                }
            })
            .collect_vec();

        let rendered_hcl = Route53Template {
            project_name,
            zones,
        }
        .render();
        let out = Artifact {
            content_type: ContentType::HCL("HCL"),
            content: rendered_hcl,
            write_path: "net/plan.tf".into(),
        };
        Ok(vec![out])
    }
}

impl Bindable for DnsProvider {
    fn bind(&self, _ctx: Rc<Context>) -> Result<(), CastError> {
        Ok(())
    }
}

impl Bootable for DnsProvider {
    fn boot(&self, _ctx: Rc<Context>) -> Result<(), CastError> {
        Ok(())
    }

    fn is_booted(&self, _ctx: Rc<Context>) -> bool {
        true
    }
}

impl Provider for DnsProvider {
    fn name(&self) -> String {
        String::from(ROUTE53_PROVIDER_NAME)
    }

    fn options(&self) -> Arc<Options> {
        self.options.clone()
    }

    fn set_options(&mut self, opts: Arc<Options>) -> Result<(), ProviderError> {
        self.options = opts;
        Ok(())
    }
}

#[derive(Serialize)]
struct Route53Template {
    project_name: String,
    zones: Vec<Zone>,
}

#[derive(Serialize)]
struct Record {
    name: String,
    target: String,
    is_apigw_target: bool,
}

#[derive(Serialize)]
struct Zone {
    name: String,
    name_snaked: String,
    records: Vec<Record>,
    options: Arc<Options>,
}

impl Template for Route53Template {
    fn render(&self) -> String {
        let mut reg = Box::new(Handlebars::new());
        reg.register_template_string("tmpl", Self::tmpl()).unwrap();
        reg.render("tmpl", &self).unwrap()
    }

    fn tmpl() -> &'static str {
        r#"# Begin Route53
{{#each zones}}
provider aws {
  alias  = "{{../project_name}}-r53-{{this.name_snaked}}"
  region = "{{options.aws_region}}"
}

data aws_route53_zone {{this.name_snaked}} {
  provider = aws.{{../project_name}}-r53-{{this.name_snaked}}
  name     = "{{this.name}}"
}
{{#each this.records}}
{{#if this.is_apigw_target}}
resource aws_acm_certificate {{this.name}} {
  provider    = aws.{{../../project_name}}-r53-{{../name_snaked}}
  domain_name = "{{this.name}}.{{../../project_name}}.{{../name}}"
  validation_method = "DNS"
}

resource aws_apigatewayv2_domain_name {{this.name}} {
  provider    = aws.{{../../project_name}}-r53-{{../name_snaked}}
  domain_name = "{{this.name}}.{{../../project_name}}.{{../name}}"

  domain_name_configuration {
    certificate_arn = aws_acm_certificate.{{this.name}}.arn
    endpoint_type   = "REGIONAL"
    security_policy = "TLS_1_2"
  }
}

resource aws_route53_record {{this.name}}_validation {
  for_each = {
    for dvo in aws_acm_certificate.{{this.name}}.domain_validation_options : dvo.domain_name => {
      name   = dvo.resource_record_name
      record = dvo.resource_record_value
      type   = dvo.resource_record_type
    }
  }

  allow_overwrite = true
  name            = each.value.name
  records         = [each.value.record]
  ttl             = 60
  type            = each.value.type
  zone_id         = data.aws_route53_zone.{{../name_snaked}}.zone_id
}

resource aws_acm_certificate_validation {{this.name}} {
  certificate_arn         = aws_acm_certificate.{{this.name}}.arn
  validation_record_fqdns = [for record in aws_route53_record.{{this.name}}_validation : record.fqdn]
}
{{/if}}
resource aws_route53_record {{this.name}} {
  provider = aws.{{../../project_name}}-r53-{{../name_snaked}}
  zone_id  = data.aws_route53_zone.{{../name_snaked}}.zone_id
  name     = "{{this.name}}.{{../../project_name}}"
  type     = "A"
  {{#unless this.is_apigw_target}}ttl      = "300"
  records  = {{{this.target}}}
  {{else}}alias {
    name                   = aws_apigatewayv2_domain_name.{{this.name}}.domain_name_configuration[0].target_domain_name
    zone_id                = aws_apigatewayv2_domain_name.{{this.name}}.domain_name_configuration[0].hosted_zone_id
    evaluate_target_health = false
  }{{/unless}}
}
{{/each}}
{{/each}}
"#
    }
}
