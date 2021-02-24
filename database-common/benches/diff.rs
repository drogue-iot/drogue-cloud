use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use drogue_cloud_database_common::models::{app::Application, diff::diff_paths};
use serde_json::json;

fn criterion_benchmark(c: &mut Criterion) {
    let now = Utc::now();

    let data = json!({
        "spec": {
            "core": {
                "disabled": false,
            },
            "credentials": {
                "credentials": [
                    { "pass": "password"},
                    { "username": {"username": "foo", "password": "pwd" }}
                ],
            },
        },
        "status": {
            "trustAnchors": [
                { "anchor": {
                    "certificate": "",
                }}
            ]
        },
    });

    c.bench_function("all equal", |b| {
        b.iter(|| {
            black_box(diff_paths(
                &black_box(Application {
                    id: "id1".to_string(),
                    labels: Default::default(),
                    annotations: Default::default(),
                    creation_timestamp: now,
                    resource_version: "12345678".to_string(),
                    generation: 0,
                    data: data.clone(),
                }),
                &black_box(Application {
                    id: "id1".to_string(),
                    labels: Default::default(),
                    annotations: Default::default(),
                    creation_timestamp: now,
                    resource_version: "12345678".to_string(),
                    generation: 0,
                    data: data.clone(),
                }),
            ));
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
