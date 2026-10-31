//! The folder subtree query shared by desktop, server and filesystem operations.
use rusqlite::Connection;

/// `WITH` clause that exposes `subtree(id)`: the folder bound to `?1` and all its descendants.
/// Callers append their own `SELECT`/`DELETE`. It uses `UNION` (not `UNION ALL`) so a
/// corrupt parent loop ends instead of recursing forever; keep that when changing it.
pub const FOLDER_SUBTREE_CTE: &str = "WITH RECURSIVE subtree(id) AS (SELECT id FROM folders WHERE id=?1 UNION SELECT f.id FROM folders f JOIN subtree s ON f.parent_id=s.id)";

/// Ids of the folder and all its descendants, ascending.
pub fn folder_subtree_ids(c: &Connection, id: i64) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = c.prepare(&format!("{FOLDER_SUBTREE_CTE} SELECT id FROM subtree ORDER BY id"))?;
    let ids = stmt.query_map([id], |r| r.get(0))?.collect();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtree_contains_root_and_descendants_only_and_ends_on_a_loop() {
        let c = crate::db::connect_in_memory().unwrap();
        c.execute_batch(
            "INSERT INTO folders(id,name,path,parent_id) VALUES
             (1,'a','/a',NULL),(2,'b','/a/b',1),(3,'c','/a/b/c',2),(4,'d','/d',NULL);",
        )
        .unwrap();
        assert_eq!(folder_subtree_ids(&c, 1).unwrap(), vec![1, 2, 3]);
        assert_eq!(folder_subtree_ids(&c, 2).unwrap(), vec![2, 3]);
        assert_eq!(folder_subtree_ids(&c, 4).unwrap(), vec![4]);
        assert_eq!(folder_subtree_ids(&c, 99).unwrap(), Vec::<i64>::new());
        // A damaged catalog may contain a parent loop; the query must still terminate.
        c.execute("UPDATE folders SET parent_id=3 WHERE id=1", []).unwrap();
        assert_eq!(folder_subtree_ids(&c, 1).unwrap(), vec![1, 2, 3]);
        let n: i64 = c
            .query_row(&format!("{FOLDER_SUBTREE_CTE} SELECT COUNT(*) FROM subtree"), [2], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 3);
    }
}
