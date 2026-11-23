// SPDX-License-Identifier: GPL-3.0-only
//! Spreading independent work over threads without changing what it makes.

/// `f` over `items` on up to `threads` threads, the results in the order of `items` (so anything made from them is the
/// same for any thread count). One thread, or a single item, runs on the calling thread.
pub(crate) fn par_map<T: Sync, R: Send>(
    items: &[T],
    threads: usize,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let workers = threads.min(items.len());
    if workers <= 1 {
        return items.iter().map(f).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<Option<R>>> =
        items.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(item) = items.get(i) else {
                        break;
                    };
                    let made = f(item);
                    *slots[i]
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(made);
                }
            });
        }
    });
    slots
        .into_iter()
        .filter_map(|m| {
            m.into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::par_map;

    #[test]
    fn par_map_returns_the_results_in_the_order_of_the_items_for_any_thread_count() {
        let items: Vec<u32> = (0..257).collect();
        let expected: Vec<u32> = items.iter().map(|n| n * 3 + 1).collect();
        for threads in [0, 1, 2, 4, 16, 1000] {
            assert_eq!(par_map(&items, threads, |n| n * 3 + 1), expected);
        }
        assert_eq!(par_map(&Vec::<u32>::new(), 4, |n| *n), [] as [u32; 0]);
    }
}
