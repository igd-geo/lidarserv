use crate::lru_cache::later::{Later, LaterSender};
use crate::lru_cache::lru::Lru;
use std::error::Error;
use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use thiserror::Error;
use tracy_client::{plot, span};

struct Page<V, E> {
    /// Every page has a unique number.
    /// This allows it to detect the ABA problem, if a page gets deleted and then re-created.
    page_number: u32,

    /// lock for synchronizing the access to the underlying file
    file: Arc<Mutex<File>>,

    /// the version number is incremented each time the value is changed, allowing
    /// changes to the value to be detected.
    version_number: u32,

    /// the last time the page was accessed. Timestamp
    last_change: Instant,

    /// the actual value stored in the page.
    /// lock for synchronizing mutable access to the data.
    data: Later<Result<Arc<V>, E>>,

    /// Indicates, if the page is currently being removed from the cache.
    ///
    /// In the "normal" state, this number is even.
    ///
    /// When a thread decides to remove this entry, it is incremented by one, making it odd.
    ///
    /// Before it can finally delete the entry from the data structure,
    /// the removing thread needs to flush the page data to disk.
    /// However, other threads could access the page value during that time.
    /// In this case, the removal needs to be cancelled, because:
    ///  1) Accessing the page will have moved it to the end of the LRU order,
    ///     however, only values at the beginning of the LRU order should be removed.
    ///  2) The accessing thread could even have changed the page data,
    ///     the changes would then not be written to disk.
    ///
    /// Cancelling the removal works by incrementing the value again. This makes the value even
    /// again, which indicates that it is not flagged for removal any more.
    ///
    /// Before the removing thread finally deletes the entry from the data structure, it checks
    /// if the value is still at the value that it set it to when starting the removal.
    /// If the value has changed, then it knows that it has to abort the removal, because some
    /// other thread accessed the page in the meantime. Only, if the value is unchanged, it proceeds
    /// to finally delete the entry.
    is_in_cleanup: u32,

    /// Easy way to tell, if the page needs to be written to disk, without having to lock the
    /// file.
    /// If a page is not dirty, then it is safe to delete without writing it to disk, first.
    dirty: bool,
}

struct File {
    /// Last version that was written to disk.
    /// If this is not equal to [Page::version_number], then the page data is "dirty" and needs
    /// to be written to disk. If both version numbers are equal, then the file is "up to date".
    /// Checking this version number before writing the file allows to resolve write-write conflicts
    /// where a later version could be overwritten with an earlier version of the file.
    version_number: u32,
}

/// Manages all existing nodes and the cache.
pub struct PageManager<P, K, V, E, D> {
    loader: P,
    inner: Mutex<PageManagerInner<K, V, E, D>>,
    wakeup: Condvar,
}

/// Inner part of the PageManager, that is protected by a mutex.
/// Holds the actual cache and the directory of all existing pages.
/// * max_size is the maximum number of pages that can be stored in the cache.
/// * num_pages_plot is a tracy plot that is updated with the current cache size.
/// * recently_used_plot is a tracy plot that is updated with the number of pages that were
struct PageManagerInner<K, V, E, D> {
    cache: Lru<K, Page<V, E>>,
    directory: D,
    max_size: usize,
    next_page_number: u32,
}

/// Responsible for loading pages to/from disk.
pub trait PageLoader {
    type Key;
    type Data;
    type Error;

    fn load(&self, key: &Self::Key) -> Result<Self::Data, Self::Error>;
    fn store(&self, key: &Self::Key, data: &Self::Data) -> Result<(), Self::Error>;
}

/// Keeps track of the list of existing pages,
/// both on disk and in the cache.
pub trait PageDirectory {
    type Key;

    /// Called every time a page is created.
    fn insert(&mut self, key: &Self::Key);

    /// Tests, if a page already exists.
    fn exists(&self, key: &Self::Key) -> bool;
}

/// Error when removing a page from the cache and writing it to disk.
/// Returns the (removed) key and value of the page, so it can be dealt with
/// (like re-inserting into the cache, or writing to a special "error-node")
#[derive(Error)]
#[error("Error at page {key:?}: {source}")]
pub struct CacheCleanupError<K: Debug, V, E: Error> {
    #[source]
    pub source: E,
    pub key: K,
    pub value: Arc<V>,
}

impl<K: Debug, V, E: Error> Debug for CacheCleanupError<K, V, E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheCleanupError")
            .field("key", &self.key)
            .field("source", &self.source)
            .finish()
    }
}

impl<P, K, V, E, D> PageManager<P, K, V, E, D>
where
    P: PageLoader<Key = K, Data = V, Error = E>,
    K: Clone + Eq + Hash + Debug,
    V: Default,
    E: Error + Clone,
    D: PageDirectory<Key = K>,
{
    /// Creates a new PageManager.
    /// Only needed when server/evaluation is started.
    pub fn new(page_loader: P, page_directory: D, max_size: usize) -> Self {
        PageManager {
            loader: page_loader,
            inner: Mutex::new(PageManagerInner {
                cache: Lru::new(),
                directory: page_directory,
                max_size,
                next_page_number: 0,
            }),
            wakeup: Condvar::new(),
        }
    }

    pub fn block_on_cache_size_external_wakeup(&self) {
        self.wakeup.notify_all();
    }

    /// Blocks until the condition function Fn(current_size, max_size)->bool returns true.
    /// This can be used to wait until the cache is empty enough to load more pages. (assuming that a second thread continuously cleans up the cache)
    /// Or, the opposite use case, to wait until it is full enough to start the cleanup process.
    pub fn block_on_cache_size<Cond>(&self, cond_fn: Cond)
    where
        Cond: Fn(usize, usize) -> bool,
    {
        let mut cache_lock = self.inner.lock().unwrap();
        while !cond_fn(cache_lock.cache.len(), cache_lock.max_size) {
            cache_lock = self.wakeup.wait(cache_lock).unwrap();
        }
    }

    /// Returns the tuple (max_size, current_size)
    pub fn size(&self) -> (usize, usize) {
        let lock = self.inner.lock().unwrap();
        (lock.max_size, lock.cache.len())
    }

    /// Returns the inner page directory.
    pub fn directory(&self) -> DirectoryGuard<'_, K, V, E, D> {
        DirectoryGuard(self.inner.lock().unwrap())
    }

    /// Used to overwrite page at key/node_id with new_value.
    #[inline]
    pub fn store(&self, key: &K, new_value: V) {
        self.store_arc(key, Arc::new(new_value))
    }

    pub fn store_arc(&self, key: &K, new_value: Arc<V>) {
        let s = span!("PageManager::store");

        // (try to) get the entry from the cache
        let mut cache_lock = self.inner.lock().unwrap();
        let cached = cache_lock.cache.touch(key);
        match cached {
            // If the page is not present in the cache yet,
            // we can insert it directly.
            // (No need to load the actual file, because we want to assign it a new value anyways)
            None => {
                let page = Page {
                    file: Arc::new(Mutex::new(File { version_number: 0 })),
                    data: Later::new(Ok(new_value)),
                    version_number: 1,
                    last_change: Instant::now(),
                    is_in_cleanup: 0,
                    page_number: cache_lock.next_page_number,
                    dirty: true,
                };
                cache_lock.directory.insert(key);
                cache_lock.next_page_number += 1;
                cache_lock.cache.insert(key.clone(), page);
                cache_lock.plot_cache_size();
                cache_lock.plot_recently_used();
                self.wakeup.notify_all();
            }

            // If the page already exists in the cache,
            // we need to create a new version for the existing entry.
            Some(page) => {
                // new version
                page.version_number += 1;

                // if the page is currently being deleted: abort that
                // (The changes we make would get lost. Also, accessing the page has moved it to
                // the end of the LRU order, so it should not be deleted any more.)
                page.is_in_cleanup += page.is_in_cleanup % 2;

                // replace data
                page.data = Later::new(Ok(new_value));

                // mark dirty
                page.dirty = true;
            }
        }
        drop(s);
    }

    #[inline]
    /// Load page. If it does not exist or an error occurs, return default page.
    /// Internally calls [Self::load]
    pub fn load_or_default(&self, key: &K) -> Result<Arc<V>, E> {
        self.load(key)
            .map(|maybe_value| maybe_value.unwrap_or_else(|| Arc::new(V::default())))
    }

    /// Load a page. If it is not in the cache, load it from disk.
    pub fn load(&self, key: &K) -> Result<Option<Arc<V>>, E> {
        let s = span!("PageManager::load");

        // (try to) get the entry from the cache
        let mut cache_lock = self.inner.lock().unwrap();
        let cached = cache_lock.cache.touch(key);
        match cached {
            None => {
                if cache_lock.directory.exists(key) {
                    // The page is not in the cache, but it exists on disk.
                    // Add a new page to the cache, with an yet unresolved Later value, to indicate
                    // that loading this page is still in progress.

                    let file = Arc::new(Mutex::new(File { version_number: 0 }));
                    let sender = LaterSender::new();
                    let later = sender.later();
                    let page = Page {
                        file: Arc::clone(&file),
                        version_number: 0,
                        last_change: Instant::now(),
                        data: later,
                        is_in_cleanup: 0,
                        page_number: cache_lock.next_page_number,
                        dirty: false,
                    };
                    cache_lock.next_page_number += 1;
                    cache_lock.cache.insert(key.clone(), page);
                    cache_lock.plot_cache_size();
                    cache_lock.plot_recently_used();
                    drop(cache_lock);
                    self.wakeup.notify_all();

                    // load file
                    let file_lock = file.lock().unwrap();
                    let value = self.loader.load(key);
                    drop(file_lock);

                    // publish the loaded data into the Later value.
                    let value = match value {
                        Ok(v) => Ok(Arc::new(v)),
                        Err(e) => Err(e),
                    };
                    sender.send(value.clone());

                    // return what we have just loaded
                    drop(s);
                    value.map(Some)
                } else {
                    // The page is not in the cache, and neither does it exist on disk.
                    // Just return an empty new page.
                    // (Will be created, once store() is called for it for the first time.)
                    drop(cache_lock);
                    drop(s);
                    Ok(None)
                }
            }

            Some(page) => {
                // If the page is already in the cache, we can return the existing value.

                // if the page is currently being deleted: abort that
                // (accessing the page has moved it to the end of the LRU order, so it should not
                // be deleted any more)
                page.is_in_cleanup += page.is_in_cleanup % 2;

                // get a snapshot of the current page data
                let data = page.data.clone();

                // unwrap the later value.
                // (this is a potentially long blocking operation, so we drop the lock first.)
                drop(cache_lock);
                let data = data.into().unwrap();
                drop(s);
                data.map(Some)
            }
        }
    }

    /// Like [Self::cleanup], just that it only removes a single page.
    /// Also, only cleans up pages, that have not been modified and can be removed directly without
    /// first writing them to disk. AS a result, this method cannot fail.
    pub fn cleanup_one_no_write(&self) {
        let mut cache_lock = self.inner.lock().unwrap();
        if cache_lock.cache.len() <= cache_lock.max_size {
            return;
        }
        let page = cache_lock
            .cache
            .iter()
            .find(|(_, page)| page.is_in_cleanup % 2 == 0 && !page.dirty);
        let key = match page {
            None => return,
            Some((k, _)) => k.clone(),
        };
        cache_lock.cache.remove(&key);
        cache_lock.plot_cache_size();
        cache_lock.plot_recently_used();
        self.wakeup.notify_all();
    }

    /// Like [Self::cleanup], just that it only removes a single page.
    pub fn cleanup_one(&self) -> Result<bool, CacheCleanupError<K, V, E>> {
        let s = span!("PageManager::cleanup_one");

        let s1 = span!("PageManager::cleanup_one - locked 1");
        let mut cache_lock = self.inner.lock().unwrap();

        // decide, if we actually need to remove anything
        if cache_lock.cache.len() <= cache_lock.max_size {
            return Ok(false);
        }

        // find a page that we can commit and remove
        let page = cache_lock
            .cache
            .iter_mut()
            .find(|(_, page)| page.is_in_cleanup % 2 == 0);
        let (key, page) = match page {
            None => return Ok(false), // when no pages are available for cleanup, this function becomes a noop.
            Some(p) => p,
        };
        let key = key.clone();

        // fast path in case the page is not dirty: we do not need to save it then.
        if !page.dirty {
            cache_lock.cache.remove(&key);
            cache_lock.plot_cache_size();
            cache_lock.plot_recently_used();
            self.wakeup.notify_all();
            return Ok(true);
        }

        // mark page for removal
        page.is_in_cleanup += 1;

        // Get a snapshot of the current page
        let Page {
            page_number,
            file,
            version_number,
            data,
            is_in_cleanup,
            ..
        } = page.clone();

        // Before flushing the page data to disk, we drop the cache lock,
        // because the disk access will take relatively long and it would not be good to block
        // the entire page cache during that time.
        drop(cache_lock);
        drop(s1);
        s.emit_text(format!("{:?}", &key).as_str());

        // resolve the Later instance to get the actual data to write
        let data = data.into().unwrap();

        // write to disk
        // if the data just indicated a loading error, all we need to do is to remove the
        // entry from the cache.
        let write_result = match &data {
            Ok(data) => {
                let mut file_lock = file.lock().unwrap();
                if version_number > file_lock.version_number {
                    file_lock.version_number = version_number;
                    self.loader.store(&key, data.as_ref())
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        };

        // acquire the lock again
        cache_lock = self.inner.lock().unwrap();
        let s3 = span!("PageManager::cleanup_one - locked 2");

        // get a reference to the page again
        // (the old one got invalid when we dropped the lock.)
        let page = match cache_lock.cache.get(&key) {
            // entry does not exist any more - this can happen very rarely, if some other thread
            // cancels the removal, and then it gets picked for removal again by a second concurrent
            // commit_remove_one operation, that ends up to be quicker.
            // Whatever, if it is already deleted our work here is done.
            // (In this case we also can ignore any errors from `write_result`, because the concurrent
            // remove operation will have taken care of the writing for us.)
            None => return Ok(true),

            Some(page) => page,
        };

        // check, that no other thread cancelled the delete while we temporarily dropped the lock
        // (If the delete got cancelled, there is also no need for the write to disk to be successful,
        // so we can ignore a potential error value in `write_result`.)
        if page.is_in_cleanup != is_in_cleanup || page.page_number != page_number {
            return Ok(true);
        }

        // DEBUG: at this point, the version MUST be the one that we have written to the file.
        // Any thread modifying the version must also abort the removal!
        debug_assert_eq!(page.version_number, version_number);

        // Finally remove it from the cache
        // At this point, the only references to parts of the page, that other threads can
        // still have are:
        // - The page data (either by cloning the Later, or the contained Arc). We don't care about
        //   this, the memory will be freed as soon as everyone has stopped using it.
        // - The File (by cloning the Arc, that it is wrapped in). It is important, that no more
        //   thread tries to write to that file, because it could interfere with threads trying
        //   to read the very same file again.
        //   However, all writes are wrapped by the condition, that the written version needs to
        //   be newer than the current version on disk. However, we just have written the last
        //   version - there is no newer version than that. Since we now remove the page from
        //   the cache, it is also impossible to create even newer versions.
        cache_lock.cache.remove(&key);
        cache_lock.plot_cache_size();
        cache_lock.plot_recently_used();
        self.wakeup.notify_all();
        drop(s3);
        drop(s);

        write_result
            .map_err(|e| CacheCleanupError {
                source: e,
                key,
                value: data.unwrap(),
            })
            .map(|()| true)
    }

    /// Writes cached pages to disk and releases them from the cache,
    /// until the cache size is below the limit.
    /// In case of an IO error, the problem page is still removed from the cache,
    /// but instead of writing it to disk, it is returned as part of the returned error. It is up to
    /// the caller to decide, how to proceed (retry by re-inserting it, store it elsewhere, ignore it, panick, ...)
    pub fn cleanup(&self) -> Result<(), CacheCleanupError<K, V, E>> {
        while self.cleanup_one()? {}
        Ok(())
    }

    /// Writes all cached pages to disk, leaving the cache empty.
    pub fn flush(&self) -> Result<(), E> {
        let _span = span!("PageManager::flush");

        // set max size to 0
        let original_max_size = {
            let mut cache_lock = self.inner.lock().unwrap();
            mem::replace(&mut cache_lock.max_size, 0)
        };

        // as a result, cleanup will run until no pages are left in the cache
        let result = match self.cleanup() {
            Ok(_) => Ok(()),
            Err(e) => {
                // re-insert the page that caused the problem
                self.store_arc(&e.key, e.value);
                Err(e.source)
            }
        };

        // restore max size
        let mut cache_lock = self.inner.lock().unwrap();
        cache_lock.max_size = original_max_size;

        result
    }
}

/// Stupid, `derive(Clone)` refuses to generate a clone implementation if V is not Clone,
/// even if that would not be necessary. (All instances of V are behind an Arc, which always
/// can be cloned.)
impl<V, E> Clone for Page<V, E>
where
    E: Clone,
{
    fn clone(&self) -> Self {
        Page {
            page_number: self.page_number,
            file: Arc::clone(&self.file),
            version_number: self.version_number,
            last_change: self.last_change,
            data: self.data.clone(),
            is_in_cleanup: self.is_in_cleanup,
            dirty: self.dirty,
        }
    }
}

impl<K, V, F, D> PageManagerInner<K, V, F, D> {
    #[inline]
    fn plot_cache_size(&self) {
        plot!("Page LRU Cache size", self.cache.len() as f64)
    }

    /// Plots the number of pages that were used in the defined time interval.
    /// Only counts pages that are still in the cache.
    #[inline]
    fn plot_recently_used(&self) {
        let interval = Duration::from_secs(1);
        let now = Instant::now();
        let mut count = 0;
        for entry in self.cache.entries.values() {
            if now - entry.data.last_change < interval {
                count += 1;
            }
        }
        plot!("Page LRU Cache recently used", count as f64);
    }
}

pub struct DirectoryGuard<'a, K, V, E, D>(MutexGuard<'a, PageManagerInner<K, V, E, D>>);

impl<K, V, E, D> Deref for DirectoryGuard<'_, K, V, E, D> {
    type Target = D;

    fn deref(&self) -> &Self::Target {
        &self.0.directory
    }
}

impl<K, V, E, D> DerefMut for DirectoryGuard<'_, K, V, E, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0.directory
    }
}
