use criterion::{criterion_group, criterion_main, Criterion};
use libalphastream::cache::{FrameCache, FrameData};

fn bench_cache_operations(c: &mut Criterion) {
    let data = FrameData {
        polystream: vec![1, 2, 3, 4],
        bitmap: Some(vec![128; 100 * 100]), // Smaller frame for isolating copying effects
        triangle_strip: Some(vec![0.0; 100]), // Smaller triangle strip data
    };

    c.bench_function("cache_insert", |b| {
        let cache = FrameCache::new(1000);
        let mut key = 0;
        b.iter(|| {
            cache.insert(std::hint::black_box(key), std::hint::black_box(data.clone()));
            key += 1;
        })
    });

    c.bench_function("cache_get_hit", |b| {
        let cache = FrameCache::new(1000);
        for i in 0..1000 {
            cache.insert(i, data.clone());
        }
        let mut key = 0;
        b.iter(|| {
            let _ = cache.get(std::hint::black_box(key % 1000));
            key += 1;
        })
    });

    c.bench_function("cache_get_miss", |b| {
        let cache = FrameCache::new(1000);
        for i in 0..1000 {
            cache.insert(i, data.clone());
        }
        let mut key = 1000;
        b.iter(|| {
            let _ = cache.get(std::hint::black_box(key));
            key += 1;
        })
    });

    c.bench_function("cache_contains", |b| {
        let cache = FrameCache::new(1000);
        for i in 0..1000 {
            cache.insert(i, data.clone());
        }
        let mut key = 0;
        b.iter(|| {
            let _ = cache.contains(&std::hint::black_box(key % 1000));
            key += 1;
        })
    });
}

fn bench_scheduler_operations(c: &mut Criterion) {
    let mut scheduler = libalphastream::scheduler::Scheduler::new();

    c.bench_function("scheduler_schedule_task", |b| {
        b.iter(|| {
            let task = libalphastream::scheduler::Task::new(std::hint::black_box(100));
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