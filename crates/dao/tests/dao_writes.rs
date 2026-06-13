use dao::{async_trait, dao, ExecuteResult, FromSqlColumn, Pool, Result, ToSqlColumn};

// --- Custom newtype for testing symmetric FromSqlColumn/ToSqlColumn ---
#[derive(Debug, Clone, PartialEq)]
struct Slug(String);

impl FromSqlColumn for Slug {
    fn from_column(value: &dao::ColumnValue) -> dao::Result<Self> {
        let s = String::from_column(value)?;
        // Validate: lowercase + hyphens only
        assert!(
            s.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
            "invalid slug: {}",
            s
        );
        Ok(Slug(s))
    }
}

impl ToSqlColumn for Slug {
    fn to_column(&self) -> dao::Result<dao::Param> {
        self.0.to_column()
    }
}

// --- Entity using custom newtype ---
#[derive(Debug, Clone, PartialEq, dao::Entity)]
#[dao(table = "articles")]
struct Article {
    #[dao(pk)]
    id: i64,
    slug: Slug,
    title: String,
}

// --- Entity for testing ---
#[derive(Debug, Clone, PartialEq, dao::Entity)]
#[dao(table = "items")]
struct Item {
    #[dao(pk)]
    id: i64,
    name: String,
    price: f64,
}

// --- Mixed trait with all annotation types ---
#[dao]
#[async_trait]
trait ItemDao {
    #[query("SELECT id, name, price FROM items WHERE id = ?")]
    async fn get_by_id(&self, id: i64) -> Result<Option<Item>>;

    #[query("SELECT id, name, price FROM items")]
    async fn get_all(&self) -> Result<Vec<Item>>;

    #[insert]
    async fn insert(&self, item: Item) -> Result<ExecuteResult>;

    #[update]
    async fn update(&self, item: Item) -> Result<ExecuteResult>;

    #[delete]
    async fn delete(&self, item: Item) -> Result<ExecuteResult>;

    #[execute("DELETE FROM items")]
    async fn delete_all(&self) -> Result<ExecuteResult>;

    #[execute("UPDATE items SET price = ? WHERE id = ?")]
    async fn set_price(&self, price: f64, id: i64) -> Result<ExecuteResult>;
}

/// Creates a temp directory + pool + creates the items table.
/// Returns (pool, TempDir) — caller must keep TempDir alive for pool to work.
async fn setup_items_db() -> (Pool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    pool.execute(
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL)",
        vec![],
    )
    .await
    .unwrap();

    (pool, dir)
}

#[tokio::test]
async fn dao_insert_and_query() {
    let (pool, _dir) = setup_items_db().await;
    let dao = ItemDao::new(pool);

    // Insert an item
    let item = Item {
        id: 1,
        name: "widget".to_string(),
        price: 9.99,
    };
    let result = dao.insert(item.clone()).await.unwrap();
    assert_eq!(result.rows_affected, 1);
    assert_eq!(result.last_insert_rowid, 1);

    // Read it back
    let fetched = dao.get_by_id(1).await.unwrap();
    assert_eq!(fetched, Some(item));
}

#[tokio::test]
async fn dao_update() {
    let (pool, _dir) = setup_items_db().await;
    let dao = ItemDao::new(pool);

    // Insert first
    let item = Item {
        id: 1,
        name: "widget".to_string(),
        price: 9.99,
    };
    dao.insert(item).await.unwrap();

    // Update
    let updated = Item {
        id: 1,
        name: "gadget".to_string(),
        price: 19.99,
    };
    let result = dao.update(updated.clone()).await.unwrap();
    assert_eq!(result.rows_affected, 1);

    // Verify
    let fetched = dao.get_by_id(1).await.unwrap();
    assert_eq!(fetched, Some(updated));
}

#[tokio::test]
async fn dao_delete() {
    let (pool, _dir) = setup_items_db().await;
    let dao = ItemDao::new(pool);

    // Insert first
    let item = Item {
        id: 1,
        name: "widget".to_string(),
        price: 9.99,
    };
    dao.insert(item.clone()).await.unwrap();

    // Delete
    let result = dao.delete(item).await.unwrap();
    assert_eq!(result.rows_affected, 1);

    // Verify gone
    let fetched = dao.get_by_id(1).await.unwrap();
    assert_eq!(fetched, None);
}

#[tokio::test]
async fn dao_execute_delete_all() {
    let (pool, _dir) = setup_items_db().await;
    let dao = ItemDao::new(pool);

    // Insert multiple items
    dao.insert(Item {
        id: 1,
        name: "a".to_string(),
        price: 1.0,
    })
    .await
    .unwrap();
    dao.insert(Item {
        id: 2,
        name: "b".to_string(),
        price: 2.0,
    })
    .await
    .unwrap();
    dao.insert(Item {
        id: 3,
        name: "c".to_string(),
        price: 3.0,
    })
    .await
    .unwrap();

    // Delete all via #[execute]
    let result = dao.delete_all().await.unwrap();
    assert_eq!(result.rows_affected, 3);

    // Verify all gone
    let all = dao.get_all().await.unwrap();
    assert!(all.is_empty());
}

#[tokio::test]
async fn dao_execute_scalar_params() {
    let (pool, _dir) = setup_items_db().await;
    let dao = ItemDao::new(pool);

    // Insert first
    dao.insert(Item {
        id: 1,
        name: "widget".to_string(),
        price: 9.99,
    })
    .await
    .unwrap();

    // Update just the price via #[execute] with scalar params
    let result = dao.set_price(42.0, 1).await.unwrap();
    assert_eq!(result.rows_affected, 1);

    // Verify
    let fetched = dao.get_by_id(1).await.unwrap();
    assert_eq!(fetched.unwrap().price, 42.0);
}

#[tokio::test]
async fn dao_mixed_operations() {
    let (pool, _dir) = setup_items_db().await;
    let dao = ItemDao::new(pool);

    // Insert multiple
    dao.insert(Item {
        id: 1,
        name: "a".to_string(),
        price: 1.0,
    })
    .await
    .unwrap();
    dao.insert(Item {
        id: 2,
        name: "b".to_string(),
        price: 2.0,
    })
    .await
    .unwrap();

    // Query all
    let all = dao.get_all().await.unwrap();
    assert_eq!(all.len(), 2);

    // Update one
    dao.update(Item {
        id: 1,
        name: "A".to_string(),
        price: 10.0,
    })
    .await
    .unwrap();

    // Execute scalar update
    dao.set_price(20.0, 2).await.unwrap();

    // Delete one
    dao.delete(Item {
        id: 1,
        name: "A".to_string(),
        price: 10.0,
    })
    .await
    .unwrap();

    // Only one left
    let remaining = dao.get_all().await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, 2);
    assert_eq!(remaining[0].price, 20.0);
}

// --- DAO for Article entity with custom newtype ---
#[dao]
#[async_trait]
trait ArticleDao {
    #[query("SELECT id, slug, title FROM articles WHERE id = ?")]
    async fn get_by_id(&self, id: i64) -> Result<Option<Article>>;

    #[insert]
    async fn insert(&self, article: Article) -> Result<ExecuteResult>;

    #[update]
    async fn update(&self, article: Article) -> Result<ExecuteResult>;
}

// --- DAO using #[execute] with entity expansion ---
#[dao]
#[async_trait]
trait ExecuteEntityDao {
    #[query("SELECT id, name, price FROM items WHERE id = ?")]
    async fn get_by_id(&self, id: i64) -> Result<Option<Item>>;

    // #[execute] with single entity param — expands to 3 positional params via ToRow
    #[execute("INSERT OR REPLACE INTO items (id, name, price) VALUES (?, ?, ?)")]
    async fn upsert(&self, item: Item) -> Result<ExecuteResult>;
}

#[tokio::test]
async fn custom_newtype_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    // Create table
    pool.execute(
        "CREATE TABLE articles (id INTEGER PRIMARY KEY, slug TEXT, title TEXT)",
        vec![],
    )
    .await
    .unwrap();

    let dao = ArticleDao::new(pool);

    // Insert with custom Slug newtype
    let article = Article {
        id: 1,
        slug: Slug("hello-world".to_string()),
        title: "Hello World".to_string(),
    };
    let result = dao.insert(article.clone()).await.unwrap();
    assert_eq!(result.rows_affected, 1);

    // Read it back — Slug goes through FromSqlColumn
    let fetched = dao.get_by_id(1).await.unwrap();
    assert_eq!(fetched, Some(article));

    // Update with new slug
    let updated = Article {
        id: 1,
        slug: Slug("updated-slug".to_string()),
        title: "Updated Title".to_string(),
    };
    dao.update(updated.clone()).await.unwrap();

    // Verify update round-trip
    let fetched = dao.get_by_id(1).await.unwrap();
    assert_eq!(fetched, Some(updated));
}

#[tokio::test]
async fn dao_execute_entity_expansion() {
    let (pool, _dir) = setup_items_db().await;
    let dao = ExecuteEntityDao::new(pool);

    // Insert via #[execute] with entity expansion
    let item = Item {
        id: 1,
        name: "widget".to_string(),
        price: 9.99,
    };
    let result = dao.upsert(item.clone()).await.unwrap();
    assert_eq!(result.rows_affected, 1);
    assert_eq!(result.last_insert_rowid, 1);

    // Verify read back
    let fetched = dao.get_by_id(1).await.unwrap();
    assert_eq!(fetched, Some(item));

    // Upsert (replace) with updated data
    let updated = Item {
        id: 1,
        name: "gadget".to_string(),
        price: 19.99,
    };
    let result = dao.upsert(updated.clone()).await.unwrap();
    assert_eq!(result.rows_affected, 1);

    // Verify updated data
    let fetched = dao.get_by_id(1).await.unwrap();
    assert_eq!(fetched, Some(updated));
}
