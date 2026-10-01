//! Разбиение кадра на горизонтальные полосы и параллельный расчёт.
//!
//! Стоимость строки у фрактала различается на порядки: строка, прошедшая по
//! «телу» множества, считает все итерации, а строка по фону вылетает за пару.
//! Поэтому полос делается заметно больше, чем потоков, и потоки разбирают их
//! из общей очереди — это дешёвая балансировка нагрузки без планировщика.

use std::sync::Mutex;

pub struct ThreadPool {
    threads: usize,
    /// Сколько полос приходится на один поток. Больше — лучше балансировка,
    /// но выше накладные расходы на синхронизацию.
    bands_per_thread: usize,
}

impl ThreadPool {
    /// `threads == 0` — определить число ядер автоматически.
    pub fn new(threads: usize) -> Self {
        let threads = if threads == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        } else {
            threads
        };
        ThreadPool {
            threads: threads.max(1),
            bands_per_thread: 4,
        }
    }

    pub fn threads(&self) -> usize {
        self.threads
    }

    /// Выполнить `f(первая_строка_полосы, полоса)` для всех строк буфера.
    ///
    /// `data` — построчный буфер длины `rows * stride`.
    pub fn for_each_band<T, F>(&self, data: &mut [T], stride: usize, f: F)
    where
        T: Send,
        F: Fn(usize, &mut [T]) + Sync,
    {
        if stride == 0 || data.is_empty() {
            return;
        }
        let rows = data.len() / stride;

        if self.threads == 1 || rows <= 1 {
            for (i, band) in data.chunks_mut(stride).enumerate() {
                f(i, band);
            }
            return;
        }

        let rows_per_band = rows.div_ceil(self.threads * self.bands_per_thread).max(1);

        let bands: Vec<(usize, &mut [T])> = data
            .chunks_mut(rows_per_band * stride)
            .enumerate()
            .map(|(i, band)| (i * rows_per_band, band))
            .collect();

        // Очередь полос: поток берёт следующую, как только освободился.
        let queue = Mutex::new(bands.into_iter());
        let workers = self.threads.min(rows);

        // Потоки создаются на кадр: при 8 потоках это доли миллисекунды,
        // зато не нужно держать живой пул и синхронизировать его состояние.
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    loop {
                        let job = queue.lock().unwrap_or_else(|e| e.into_inner()).next();
                        let Some((first_row, band)) = job else { break };
                        for (i, row) in band.chunks_mut(stride).enumerate() {
                            f(first_row + i, row);
                        }
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_every_row_exactly_once() {
        for threads in [1, 2, 3, 8, 16] {
            let pool = ThreadPool::new(threads);
            let stride = 7;
            let rows = 53;
            let mut data = vec![0u32; stride * rows];
            pool.for_each_band(&mut data, stride, |row, slice| {
                for (x, cell) in slice.iter_mut().enumerate() {
                    *cell = (row * stride + x) as u32 + 1;
                }
            });
            for (i, v) in data.iter().enumerate() {
                assert_eq!(*v, i as u32 + 1, "threads = {threads}, i = {i}");
            }
        }
    }

    #[test]
    fn handles_degenerate_input() {
        let pool = ThreadPool::new(4);
        let mut empty: Vec<u32> = Vec::new();
        pool.for_each_band(&mut empty, 4, |_, _| panic!("не должно вызываться"));
        let mut one = vec![0u32; 4];
        pool.for_each_band(&mut one, 4, |row, slice| {
            assert_eq!(row, 0);
            slice[0] = 9;
        });
        assert_eq!(one[0], 9);
    }

    #[test]
    fn detects_available_parallelism() {
        assert!(ThreadPool::new(0).threads() >= 1);
    }
}
