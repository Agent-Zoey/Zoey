use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use zoey_plugin_knowledge::graph::KnowledgeGraph;
use zoey_plugin_knowledge::retrieval::HybridRetriever;
use tokio::runtime::Runtime;

fn bench_hybrid_retrieval(c: &mut Criterion) {
    let mut group = c.benchmark_group("knowledge_hybrid_retrieval");

    for &docs in &[100, 1000, 5000] {
        group.bench_with_input(BenchmarkId::from_parameter(docs), &docs, |b, &docs| {
            let corpus: Vec<String> = (0..docs)
                .map(|i| format!("Doc {} about topic {} and some details", i, i % 10))
                .collect();
            let graph = KnowledgeGraph::new("bench");
            let retriever = HybridRetriever::new(graph, corpus);
            b.iter(|| {
                let rt = Runtime::new().unwrap();
                let _ = rt.block_on(async {
                    retriever
                        .search(black_box("topic details"), black_box(10))
                        .await
                        .unwrap()
                });
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_hybrid_retrieval);
criterion_main!(benches);
