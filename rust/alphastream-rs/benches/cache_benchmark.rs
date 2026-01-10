use criterion::{black_box, criterion_group, criterion_main, Criterion};
use libalphastream::cache::{FrameCache, FrameData};

fn bench_cache_operations(c: &mut Criterion) {
    let cache = FrameCache::new(1000);
    let data = FrameData {
        polystream: vec![1, 2, 3, 4],
        bitmap: Some(vec![128; 1920 * 1080]), // Full HD frame
        triangle_strip: Some(vec![0.0; 1000]), // Triangle strip data
    };

    c.bench_function("cache_insert", |b| {
        b.iter(|| {
            cache.insert(black_box(42), black_box(data.clone()));
        })
    });

    // Pre-populate for get benchmark
    cache.insert(42, data);

    c.bench_function("cache_get", |b| {
        b.iter(|| {
            let _ = cache.get(black_box(42));
        })
    });

    c.bench_function("cache_contains", |b| {
        b.iter(|| {
            let _ = cache.contains(&black_box(42));
        })
    });
}

fn bench_scheduler_operations(c: &mut Criterion) {
    let mut scheduler = libalphastream::scheduler::Scheduler::new();

    c.bench_function("scheduler_schedule_task", |b| {
        b.iter(|| {
            let task = libalphastream::scheduler::Task::new(black_box(100));
            scheduler.schedule_task(task);
        })
    });

    // Pre-populate tasks
    for i in 0..10 {
        let task = libalphastream::scheduler::Task::new(i);
        scheduler.schedule_task(task);
    }

    c.bench_function("scheduler_next_task", |b| {
        b.iter(|| {
            let _ = scheduler.next_task();
        })
    });
}

criterion_group!(benches, bench_cache_operations, bench_scheduler_operations);
criterion_main!(benches);