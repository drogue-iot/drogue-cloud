mod error;

use crate::error::ServiceError;
use actix_web::{middleware, post, web, App, HttpRequest, HttpResponse, HttpServer};
use chrono::Utc;
use cloudevents::event::Data;
use cloudevents::AttributesReader;
use cloudevents_sdk_actix_web::HttpRequestExt;
use envconfig::Envconfig;
use influxdb::{Client, InfluxDbWriteable, Timestamp, Type, WriteQuery};
use jsonpath_lib::Selector;
use serde_json::Value;
use std::collections::HashMap;

fn add_to_query<F>(
    mut query: WriteQuery,
    processor: &HashMap<String, Path>,
    json: &Value,
    f: F,
) -> Result<(WriteQuery, usize), ServiceError>
where
    F: Fn(WriteQuery, &String, Type) -> WriteQuery,
{
    let mut num = 0;

    let mut f = |query, field, value| {
        num += 1;
        f(query, field, value)
    };

    for (ref field, ref path) in processor {
        let sel = Selector::default()
            .compiled_path(&path.node)
            .value(&json)
            .select()
            .map_err(|err| ServiceError::SelectorError {
                details: err.to_string(),
            })?;

        query = match sel.as_slice() {
            // no value, don't add
            [] => Ok(query),
            // single value, process
            [v] => match v {
                Value::String(s) => Ok(f(query, field, Type::Text(s.clone()))),
                Value::Bool(b) => Ok(f(query, field, Type::Boolean(*b))),
                Value::Number(n) => {
                    if let Some(n) = n.as_u64() {
                        Ok(f(query, field, Type::UnsignedInteger(n)))
                    } else if let Some(n) = n.as_i64() {
                        Ok(f(query, field, Type::SignedInteger(n)))
                    } else if let Some(n) = n.as_f64() {
                        Ok(f(query, field, Type::Float(n)))
                    } else {
                        Err(ServiceError::PayloadParseError {
                            details: format!(
                                "Unknown numeric type - path: {}, value: {:?}",
                                path.path, n
                            ),
                        })
                    }
                }
                _ => Err(ServiceError::PayloadParseError {
                    details: format!(
                        "Invalid value type selected - path: {}, value: {:?}",
                        path.path, v
                    ),
                }),
            },
            // multiple values, error
            [..] => Err(ServiceError::SelectorError {
                details: format!("Selector found more than one value: {}", sel.len()),
            }),
        }?;
    }

    Ok((query, num))
}

fn add_values(
    query: WriteQuery,
    processor: &Processor,
    json: &Value,
) -> Result<(WriteQuery, usize), ServiceError> {
    add_to_query(query, &processor.fields, json, |query, field, value| {
        query.add_field(field, value)
    })
}

fn add_tags(
    query: WriteQuery,
    processor: &Processor,
    json: &Value,
) -> Result<(WriteQuery, usize), ServiceError> {
    add_to_query(query, &processor.tags, json, |query, field, value| {
        query.add_tag(field, value)
    })
}

fn parse_payload(data: Option<&Data>) -> Result<Value, ServiceError> {
    match data {
        Some(Data::Json(value)) => Ok(value.clone()),
        Some(Data::String(s)) => {
            serde_json::from_str::<Value>(&s).map_err(|err| ServiceError::PayloadParseError {
                details: err.to_string(),
            })
        }

        Some(Data::Binary(b)) => {
            serde_json::from_slice::<Value>(&b).map_err(|err| ServiceError::PayloadParseError {
                details: err.to_string(),
            })
        }
        _ => Err(ServiceError::PayloadParseError {
            details: "Unknown event payload".to_string(),
        }),
    }
}

#[post("/")]
async fn forward(
    req: HttpRequest,
    payload: web::Payload,
    processor: web::Data<Processor>,
) -> Result<HttpResponse, actix_web::Error> {
    let event = req.to_event(payload).await?;

    log::info!("Received Event: {:?}", event);

    let data: Option<&Data> = event.data();

    let timestamp = event.time().cloned().unwrap_or_else(Utc::now);
    let timestamp = Timestamp::from(timestamp);

    let query = timestamp.into_query(processor.table.clone());

    // process values with payload only

    let json = parse_payload(data)?;
    let (query, num) = add_values(query, &processor, &json)?;

    // create full events JSON for tags

    let event_json = serde_json::to_value(event)?;
    let (query, _) = add_tags(query, &processor, &event_json)?;

    // execute query

    if num > 0 {
        let result = processor.client.query(&query).await;

        // process result

        log::info!("Result: {:?}", result);

        match result {
            Ok(_) => Ok(HttpResponse::Accepted().finish()),
            Err(e) => Ok(HttpResponse::InternalServerError().body(e.to_string())),
        }
    } else {
        Ok(HttpResponse::NoContent().finish())
    }
}

#[derive(Envconfig, Clone, Debug)]
struct InfluxDb {
    #[envconfig(from = "INFLUXDB_URI")]
    pub uri: String,
    #[envconfig(from = "INFLUXDB_DATABASE")]
    pub db: String,
    #[envconfig(from = "INFLUXDB_USERNAME")]
    pub user: String,
    #[envconfig(from = "INFLUXDB_PASSWORD")]
    pub password: String,
    #[envconfig(from = "INFLUXDB_TABLE")]
    pub table: String,
}

#[derive(Envconfig, Clone, Debug)]
struct Config {
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
}

#[derive(Debug, Clone)]
struct Path {
    pub path: String,
    pub node: jsonpath_lib::parser::Node,
}

#[derive(Debug, Clone)]
struct Processor {
    pub client: Client,
    pub table: String,
    pub fields: HashMap<String, Path>,
    pub tags: HashMap<String, Path>,
}

#[actix_rt::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let influx = InfluxDb::init_from_env()?;
    let client = Client::new(influx.uri, influx.db).with_auth(influx.user, influx.password);

    let config = Config::init_from_env()?;
    let max_json_payload_size = config.max_json_payload_size;

    let mut fields = HashMap::new();
    let mut tags = HashMap::new();

    for (key, value) in std::env::vars() {
        if let Some(field) = key.strip_prefix("FIELD_") {
            log::info!("Adding field - {} -> {}", field, value);
            let node = jsonpath_lib::Parser::compile(&value)
                .map_err(|err| anyhow::anyhow!("Failed to parse JSON path: {}", err))?;
            fields.insert(field.to_lowercase(), Path { path: value, node });
        } else if let Some(tag) = key.strip_prefix("TAG_") {
            log::info!("Adding tag - {} -> {}", tag, value);
            let node = jsonpath_lib::Parser::compile(&value)
                .map_err(|err| anyhow::anyhow!("Failed to parse JSON path: {}", err))?;
            tags.insert(tag.to_lowercase(), Path { path: value, node });
        }
    }

    let processor = Processor {
        client,
        table: influx.table,
        fields,
        tags,
    };

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .data(processor.clone())
            .service(forward)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
