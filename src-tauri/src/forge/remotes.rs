//! Which git remote each folder's forge panel reads.
//!
//! **Not a field of [`ForgePanelSettings`](super::settings::ForgePanelSettings)**,
//! and that separation is the point of this module. The two have opposite write
//! patterns: the picker saves this one on every click, while the panel settings
//! are a blob a dialog rewrites WHOLESALE — "use global defaults" saves by
//! dropping a folder's whole row. Sharing a blob made the picker's write detach
//! every other setting from the global row, and made the dialog's drop take the
//! selection with it: a choice the user watched succeed died without a word.
//!
//! One key, one map: the folders that have chosen a remote. **No global row**,
//! unlike the panel settings — a remote names a git remote that exists in ONE
//! folder's worktree, so there is nothing for an "all folders" default to mean.
//! A folder with no entry reads the historical `origin`; absence is the default
//! answer, so there is no third state to keep in sync with the entry itself.
//!
//! Stored as ONE JSON object in `app_metadata`. A save is a read-modify-write of
//! the single blob — the same shape the panel settings use — so a save against
//! one folder carries the others through untouched rather than replacing them
//! with whatever the saving client happened to be holding.

use std::collections::BTreeMap;

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::db::error::DbError;
use crate::db::service::app_metadata_service;

/// `app_metadata` key holding the whole store.
const REMOTES_KEY: &str = "forge_panel_remotes";

/// Every folder's selection at once.
///
/// Keyed by folder id: a folder with no entry is on the default remote, so the
/// map holds only the folders that have chosen one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeRemoteStore {
    #[serde(default)]
    pub folders: BTreeMap<i32, String>,
}

impl ForgeRemoteStore {
    /// The name this folder is set to, or `None` for the default.
    pub fn selected(&self, folder_id: i32) -> Option<&str> {
        self.folders.get(&folder_id).map(String::as_str)
    }

    /// Apply one folder's save. Pure, so the arithmetic can be tested without a
    /// database — [`save`] is this plus the read and the write.
    ///
    /// A blank or absent name CLEARS the entry (back to the default) rather
    /// than storing an empty string the resolver would then have to know to
    /// ignore — the same normalization [`super::settings`] applies to its own
    /// option fields.
    fn apply(&mut self, folder_id: i32, remote: Option<String>) {
        match trim(remote) {
            Some(name) => {
                self.folders.insert(folder_id, name);
            }
            None => {
                self.folders.remove(&folder_id);
            }
        }
    }
}

fn trim(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Read every folder's selection. Never fails on content: a blob this build
/// cannot parse (hand-edited, or written by a future one) reads as empty — the
/// default remote — rather than taking down every forge read in every folder.
pub async fn load(conn: &DatabaseConnection) -> Result<ForgeRemoteStore, DbError> {
    let raw = app_metadata_service::get_value(conn, REMOTES_KEY).await?;
    Ok(decode(raw.as_deref()).unwrap_or_default())
}

/// The one folder's selection — what the panel's resolution actually asks for.
pub async fn load_selected(
    conn: &DatabaseConnection,
    folder_id: i32,
) -> Result<Option<String>, DbError> {
    Ok(load(conn).await?.selected(folder_id).map(str::to_string))
}

/// Write ONE folder's selection and hand back every folder's as stored.
pub async fn save(
    conn: &DatabaseConnection,
    folder_id: i32,
    remote: Option<String>,
) -> Result<ForgeRemoteStore, DbError> {
    let mut store = load(conn).await?;
    store.apply(folder_id, remote);
    let encoded = serde_json::to_string(&store)
        .map_err(|e| DbError::Validation(format!("remote selection not serializable: {e}")))?;
    app_metadata_service::upsert_value(conn, REMOTES_KEY, &encoded).await?;
    Ok(store)
}

/// Decode a stored blob, or `None` for anything this build cannot read.
fn decode(raw: Option<&str>) -> Option<ForgeRemoteStore> {
    serde_json::from_str(raw?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(pairs: &[(i32, &str)]) -> ForgeRemoteStore {
        let mut store = ForgeRemoteStore::default();
        for (id, name) in pairs {
            store.apply(*id, Some((*name).to_string()));
        }
        store
    }

    /// A folder with nothing saved is on the default, not on a stored empty
    /// string the resolver would have to filter out.
    #[test]
    fn an_untouched_folder_has_no_selection_of_its_own() {
        let store = ForgeRemoteStore::default();
        assert_eq!(store.selected(1), None);
        assert_eq!(store_with(&[(1, "upstream")]).selected(2), None);
    }

    /// The picker's own name goes in trimmed — a hand-edited blob must not
    /// resolve against a remote whose name is spelled with spaces around it.
    /// A blank or absent name CLEARS the entry: that is how the picker's
    /// "default (origin)" answer is stored.
    #[test]
    fn saving_trims_the_name_and_clearing_removes_the_entry() {
        let mut store = ForgeRemoteStore::default();
        store.apply(1, Some("  upstream  ".into()));
        assert_eq!(store.selected(1), Some("upstream"));

        store.apply(1, Some("   ".into()));
        assert_eq!(store.selected(1), None);
        assert!(store.folders.is_empty(), "blank is a clear, not an entry");

        store.apply(1, Some("upstream".into()));
        store.apply(1, None);
        assert_eq!(store.selected(1), None);
        // Idempotent: clearing an unset folder is fine.
        store.apply(1, None);
        assert!(store.folders.is_empty());
    }

    /// One folder's save must not rewrite another's — the whole store is
    /// written back on every save, so this is the one way a picker click could
    /// silently move a different folder.
    #[test]
    fn a_save_against_one_folder_leaves_the_others_untouched() {
        let mut store = store_with(&[(1, "upstream"), (2, "backup")]);
        store.apply(1, Some("origin".into()));
        assert_eq!(store.selected(2), Some("backup"));
        store.apply(1, None);
        assert_eq!(store.selected(2), Some("backup"));
    }

    /// JSON has no integer keys, so folder ids go out as strings — and a store
    /// that could not read its own output would lose every selection on the
    /// next load.
    #[test]
    fn folder_keys_survive_the_json_round_trip() {
        let store = store_with(&[(42, "upstream")]);
        let encoded = serde_json::to_string(&store).expect("serializable");
        assert!(encoded.contains("\"42\""), "{encoded}");
        assert_eq!(decode(Some(&encoded)).expect("decodes"), store);
    }

    /// Anything that is not JSON at all is the default, not an error — this
    /// read runs on every forge operation, in every folder.
    #[test]
    fn a_blob_this_build_cannot_parse_reads_as_empty() {
        assert_eq!(decode(None).unwrap_or_default(), ForgeRemoteStore::default());
        assert_eq!(decode(Some("not json")).unwrap_or_default(), ForgeRemoteStore::default());
        // A shape from a future build keeps the fields this one knows.
        let future = decode(Some(r#"{"folders":{"7":"upstream"},"scope":"worktree"}"#))
            .expect("unknown fields are ignored");
        assert_eq!(future.selected(7), Some("upstream"));
    }

    /// Through a real database, because [`save`] is a READ-MODIFY-WRITE: the
    /// pure tests above prove the arithmetic, and this proves the load and the
    /// store are the same blob — one key typo and every save would land on
    /// something the next load never reads.
    #[tokio::test]
    async fn selections_persist_independently_across_saves() {
        let db = crate::db::test_helpers::fresh_in_memory_db().await;

        // Nothing stored is the default, not an error.
        assert_eq!(load(&db.conn).await.expect("empty"), ForgeRemoteStore::default());

        save(&db.conn, 3, Some("upstream".into())).await.expect("folder 3");
        save(&db.conn, 4, Some("backup".into())).await.expect("folder 4");
        let store = load(&db.conn).await.expect("reload");
        assert_eq!(store.selected(3), Some("upstream"));
        assert_eq!(store.selected(4), Some("backup"));
        assert_eq!(load_selected(&db.conn, 3).await.expect("selected"), Some("upstream".into()));
        assert_eq!(load_selected(&db.conn, 5).await.expect("unset"), None);

        // Clearing one folder leaves the other alone.
        save(&db.conn, 3, None).await.expect("clear folder 3");
        assert_eq!(load(&db.conn).await.expect("reload").selected(3), None);
        assert_eq!(load_selected(&db.conn, 4).await.expect("folder 4"), Some("backup".into()));
    }
}
