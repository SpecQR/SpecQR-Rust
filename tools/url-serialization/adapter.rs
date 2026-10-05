use specqr::{gs1 as g,json::{self,Value as V},Result};
use std::io::{self,BufRead};
fn obj<const N:usize>(v:[(&str,V);N])->V{V::object(v)}
fn el(e:&g::Element)->V{obj([("ai",e.ai().into()),("value",e.value().into())])}
fn els(e:&[g::Element])->V{V::array(e.iter().map(el))}
fn info(e:&g::AiInfo)->V{obj([("ai",e.ai().into()),("label",e.label().into()),("length",obj([("type",e.length().kind().into()),("exact",e.length().exact().into()),("min",e.length().min().into()),("max",e.length().max().into())])),("valueKind",e.value_kind().into()),("checkDigitRule",e.check_digit_rule().into()),("digitalLinkRole",e.digital_link_role().into()),("separator",e.separator().into()),("digitalLinkPathForPrimary",e.digital_link_path_for_primary().map(|x|V::array(x.iter().map(|s|(*s).into()))).unwrap_or(V::Null))])}
fn issues(xs:&[g::ValidationIssue])->V{V::array(xs.iter().map(|x|obj([("code",x.code().into()),("message",x.message().into()),("reason",x.reason().into()),("count",x.count().into())])))}
fn val(v:g::ValidationResult)->V{obj([("ok",v.ok().into()),("elements",v.elements().map(els).unwrap_or(V::Null)),("hasSeparators",v.has_separators().into()),("errors",issues(v.errors())),("warnings",issues(v.warnings()))])}
fn link(v:&g::DigitalLinkParseResult)->V{obj([("elements",els(v.elements())),("primary",el(v.primary())),("pathElements",els(v.path_elements())),("queryElements",els(v.query_elements())),("unknownQuery",V::array(v.unknown_query().iter().map(|q|obj([("key",q.key().into()),("value",q.value().into())]))))])}
fn call(r:V)->Result<V>{
 let s=r.get("input").and_then(V::as_str).unwrap_or("");let mut vo=g::ValidationOptions::default();let mut lo=g::DigitalLinkOptions::default();
 if let Some(o)=r.get("options"){
  if let Some(v)=o.get("context"){vo=vo.with_context(v.as_str().unwrap());}if let Some(v)=o.get("collectAllErrors"){vo=vo.with_collect_all_errors(v.as_bool().unwrap());}if let Some(v)=o.get("allowUnsupportedAi"){vo=vo.with_allow_unsupported_ai(v.as_bool().unwrap());}
  if let Some(v)=o.get("baseUrl"){lo=lo.with_base_url(v.as_str().unwrap());}if let Some(v)=o.get("primaryAi"){lo=lo.with_primary_ai(v.as_str().unwrap());}if let Some(v)=o.get("pathAis"){lo=lo.with_path_ais(v.as_array().unwrap().iter().map(|x|x.as_str().unwrap().to_owned()).collect());}if let Some(v)=o.get("unknownQuery"){lo=lo.with_unknown_query(v.as_str().unwrap());}if let Some(v)=o.get("normalize"){lo=lo.with_normalize(v.as_bool().unwrap());}if let Some(v)=o.get("mode"){lo=lo.with_mode(v.as_str().unwrap());}
 }
 let e:Vec<g::Element>=r.get("elements").map(|x|x.as_array().unwrap().iter().map(|v|g::Element::new(v.get("ai").unwrap().as_str().unwrap(),v.get("value").unwrap().as_str().unwrap())).collect()).unwrap_or_default();
 Ok(match r.get("op").unwrap().as_str().unwrap(){
 "dictionary"=>V::array(g::get_supported_ais().iter().map(info)),"info"=>g::get_ai_info(s).map(info).unwrap_or(V::Null),
 "checkDigit"=>g::calculate_check_digit(s)?.into(),"validateCheckDigit"=>g::validate_check_digit(s)?.into(),"gtinDigit"=>g::calculate_gtin_check_digit(s)?.into(),"gtinAppend"=>g::append_gtin_check_digit(s)?.into(),"gtinValidate"=>g::validate_gtin_check_digit(s)?.into(),"ssccDigit"=>g::calculate_sscc_check_digit(s)?.into(),"ssccAppend"=>g::append_sscc_check_digit(s)?.into(),"ssccValidate"=>g::validate_sscc_check_digit(s)?.into(),
 "human"=>els(&g::from_human_readable(s)?),"create"=>g::to_element_string(&e)?.into(),"raw"=>{let v=g::parse_element_string(s)?;obj([("elements",els(v.elements())),("hasSeparators",v.has_separators().into())])},"validateElements"=>val(g::validate_elements_with_options(&e,&vo)),"validateRaw"=>val(g::validate_element_string_with_options(s,&vo)),
 "linkCreate"=>g::create_digital_link_with_options(&e,&lo)?.into(),"linkParse"=>link(&g::parse_digital_link_with_options(s,&lo)?),"linkNormalize"=>g::normalize_digital_link_with_options(s,&lo)?.into(),"linkValidate"=>{let v=g::validate_digital_link_with_options(s,&lo);obj([("ok",v.ok().into()),("result",v.result().map(link).unwrap_or(V::Null)),("errors",issues(v.errors())),("warnings",issues(v.warnings()))])},_=>panic!("unknown operation")})
}
fn main(){for l in io::stdin().lock().lines(){let r=json::parse(&l.unwrap()).unwrap();let v=match call(r){Ok(x)=>x,Err(e)=>obj([("throws",obj([("code",e.code().into()),("message",e.message().into())]))])};println!("{}",json::stringify(&v).unwrap());}}
