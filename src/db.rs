use crate::config::{Config, SqlPathConfig};
use crate::noti::Announcement;

use blake3::hash;
use log::{debug, info};
use std::fs;
use tokio_rusqlite::Connection;

pub struct Storage {
    pub conn: Connection,
}

impl Storage {
    /// Returns SQL files' path for announcement.
    fn ann_sqlpath_config() -> &'static SqlPathConfig {
        let conf = Config::instance();
        let sql_path = conf
            .database
            .sqlpath
            .get("announcement")
            .expect("Failed to get database settings for announcement.");
        sql_path
    }

    // Returns if the given announcement is new item.
    // Return
    // true - hash string: the item is not in database.
    // false - None: the item is already in the database.
    pub async fn is_new(&self, announcement: &Announcement) -> bool {
        let ann_hash = Self::calculate_hash(announcement);
        let sql_path = Self::ann_sqlpath_config();
        let exist_sql = fs::read_to_string(sql_path.check_exist.clone())
            .expect("Failed to read from existence checking sql file.");

        // Check if the database already has the ann_hash.
        let result = self
            .conn
            .call(move |c| {
                let mut stmt = c.prepare(&exist_sql)?;
                // query_row returns the first column of the first row
                let exists: bool = stmt.query_row([ann_hash.as_str()], |row| row.get(0))?;

                if exists == true {
                    info!("Hash:{} already in the database, ignored.", ann_hash);
                    Ok(false)
                } else {
                    debug!("New item hash:{}", ann_hash);
                    Ok(true)
                }
            })
            .await;
        result.unwrap_or(false)
    }

    pub async fn mark_as_notified(
        &self,
        announcement: &Announcement,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let title = announcement.title.clone();
        let content = announcement.content.clone();
        let hash = Self::calculate_hash(announcement);
        let sql_path = Self::ann_sqlpath_config();
        let upsert_sql = fs::read_to_string(sql_path.upsert.clone())?;

        // Create a hash for the given announcement.
        self.conn
            .call(move |c| {
                c.execute(&upsert_sql, [&title, &content, &hash])?;

                Ok(())
            })
            .await?;

        Ok(())
    }

    pub async fn new(database_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(database_name).await?;
        // TODO: Consider to embedding the sql using include_str()
        let sql_path = Self::ann_sqlpath_config();
        let setup_sql = fs::read_to_string(sql_path.create_table.clone())?;

        // Do initialization and create the table if not exist.
        conn.call(move |c| {
            c.execute(&setup_sql, [])?;
            Ok(())
        })
        .await?;

        Ok(Self { conn })
    }

    // Calculate hash.
    fn calculate_hash(ann: &Announcement) -> String {
        // Use post URL if available, email otherwise.
        let resource = match &ann.post_url {
            Some(url) => url,
            None => &ann.email.clone().unwrap(),
        };

        // Represents the announcement by combining information.
        let ann_info = match &ann.post_id {
            Some(id) => format!("{}{}{}{}", ann.title, ann.content, id, resource),
            None => format!("{}{}{}", ann.title, ann.content, resource),
        };

        hash(ann_info.as_bytes()).to_hex().to_string()
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use crate::collectors::{PostSource, convert_to_date};
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_basic_hash_calc() {
        let exp_hash_string = "d44433ccde9a550592efdde60f98487520daca3b6153e819526462bd69c46838";
        let ann = Announcement {
            id: None,
            post_id: None,
            post_source: PostSource::KSTARTUP,
            post_url: Some("http://www.example.com".to_string()),
            email: None,
            title: "some title".to_string(),
            content: "some content".to_string(),
            begin_date: convert_to_date("20260101"),
            end_date: convert_to_date("20261231"),
        };

        // Hash for 'hello'
        let ann_hash = Storage::calculate_hash(&ann);
        assert_eq!(exp_hash_string, ann_hash);
    }

    // A utility fixture for data creation
    async fn setup_storage() -> (NamedTempFile, Storage) {
        // Create a sqlite with a temp file file.
        let tmp = NamedTempFile::new().expect("Failed to create a temp file");
        // Canonicalize resolves the /tmp -> /private/tmp symlink on macOS so
        // SQLite doesn't see a path mismatch and raise SQLITE_READONLY_DBMOVED.
        let db_path = tmp
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();

        // Initializing storage should create a database file.
        let storage = Storage::new(&db_path)
            .await
            .expect("Failed to create a storage");

        (tmp, storage)
    }

    #[tokio::test]
    async fn test_database_creates_file_and_table() {
        // Setup a database.
        let (tmp, storage) = setup_storage().await;
        // String object in the function would be invalidated upon function exit.
        let db_path = tmp.path().to_string_lossy().to_string();

        // Check sqlite data base file.
        assert!(std::path::Path::new(&db_path).exists());

        // Check target table exists.
        let count: i64 = storage.conn.call(|c| -> std::result::Result<i64, tokio_rusqlite::Error> {
            let mut stmt = c
                .prepare(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='announcements';",
                )
                .map_err(tokio_rusqlite::Error::from)?;

            stmt.query_row([], |row| row.get(0))
                .map_err(tokio_rusqlite::Error::from)
        })
        .await
        .expect("Failed to query row");
        assert!(count > 0);
    }

    #[tokio::test]
    async fn test_new_item_in_database() {
        // Setup a database.
        let (_tmp, storage) = setup_storage().await;

        // An example announcement.
        let ann1 = Announcement {
            id: None,
            post_id: None,
            post_source: PostSource::KSTARTUP,
            post_url: Some("http://www.example.com".to_string()),
            email: None,
            title: "some title".to_string(),
            content: "some content".to_string(),
            begin_date: convert_to_date("20260101"),
            end_date: convert_to_date("20261231"),
        };
        // Deep copy of the ann1.
        let ann2 = ann1.clone();

        // Make sure the item is not in the database.
        assert_eq!(true, storage.is_new(&ann1).await);
        assert_eq!(true, storage.is_new(&ann2).await);

        // Mark the item as notified by adding it to the database.
        storage
            .mark_as_notified(&ann1)
            .await
            .expect("Failed to mark notified.");

        // Check if it can be shown as already added.
        assert_eq!(false, storage.is_new(&ann1).await);
        assert_eq!(false, storage.is_new(&ann2).await);
    }
}
