use axum::{
    extract::{State, Query},
    response::IntoResponse,
    routing::get,
    Router,
};
use askama::Template;
use sqlx::{Row, Column};

use super::auth::WebClaims;
use crate::app::AppState;

#[derive(serde::Deserialize)]
pub struct DatabaseQuery {
    pub table: Option<String>,
}

#[derive(Clone, serde::Serialize)]
pub struct TableRenderItem {
    pub name: String,
    pub is_active: bool,
}

#[derive(Clone, serde::Serialize)]
pub struct ColumnDef {
    pub field: String,
    pub type_str: String,
    pub null_allowed: String,
    pub key: String,
    pub default_val: Option<String>,
    pub extra: String,
}

#[derive(Template)]
#[template(path = "database.html")]
struct DatabaseTemplate {
    tables: Vec<TableRenderItem>,
    selected_table: Option<String>,
    columns: Vec<ColumnDef>,
    headers: Vec<String>,
    records: Vec<Vec<String>>,
}
crate::impl_into_response!(DatabaseTemplate);

pub fn router() -> Router<AppState> {
    Router::new().route("/database", get(database_page))
}

async fn database_page(
    _claims: WebClaims,
    State(state): State<AppState>,
    Query(query): Query<DatabaseQuery>,
) -> impl IntoResponse {
    // 1. 全テーブルの取得 (CAST(table_name AS CHAR) を用いて確実に String としてマッピング可能にする)
    let tables_rows = sqlx::query("SELECT CAST(table_name AS CHAR) FROM information_schema.tables WHERE table_schema = DATABASE()")
        .fetch_all(&state.db)
        .await;

    let mut tables_names = Vec::new();
    match tables_rows {
        Ok(rows) => {
            for row in rows {
                let name_opt = row.try_get::<String, _>(0)
                    .or_else(|_| row.try_get::<String, _>("table_name"))
                    .or_else(|_| {
                        row.try_get::<Vec<u8>, _>(0)
                            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                    });
                
                if let Ok(name) = name_opt {
                    tables_names.push(name);
                }
            }
        }
        Err(e) => {
            tracing::error!("データベーステーブル一覧の取得に失敗: {}", e);
        }
    }

    // テーブル一覧をソートして見やすくする
    tables_names.sort();

    let mut selected_table = query.table.filter(|t| tables_names.contains(t));
    
    // テーブルが未指定で、テーブルが存在する場合は最初のテーブルをデフォルトにする
    if selected_table.is_none() && !tables_names.is_empty() {
        selected_table = Some(tables_names[0].clone());
    }

    // tables_names から TableRenderItem へ変換
    let mut tables = Vec::new();
    for name in tables_names {
        let is_active = selected_table.as_ref().map(|sel| sel == &name).unwrap_or(false);
        tables.push(TableRenderItem {
            name,
            is_active,
        });
    }

    let mut columns = Vec::new();
    let mut headers = Vec::new();
    let mut records = Vec::new();

    if let Some(ref table_name) = selected_table {
        // 安全なテーブル名に対してのみクエリを実行（ホワイトリスト検証済）
        
        // 2. カラム構造の取得 (DESCRIBE + 各種フォールバックによる確実なデシリアライズ)
        let col_query = format!("DESCRIBE `{}`", table_name);
        let columns_rows = sqlx::query(&col_query)
            .fetch_all(&state.db)
            .await;

        match columns_rows {
            Ok(rows) => {
                for row in rows {
                    let field = get_string_value(&row, "Field", 0);
                    let type_str = get_string_value(&row, "Type", 1);
                    let null_allowed = get_string_value(&row, "Null", 2);
                    let key = get_string_value(&row, "Key", 3);
                    let default_val = get_option_string_value(&row, "Default", 4);
                    let extra = get_string_value(&row, "Extra", 5);

                    columns.push(ColumnDef {
                        field,
                        type_str,
                        null_allowed,
                        key,
                        default_val,
                        extra,
                    });
                }
            }
            Err(e) => {
                tracing::error!("テーブル '{}' のカラム構造取得に失敗: {}", table_name, e);
            }
        }

        // 3. レコードデータの取得 (SELECT * LIMIT 100)
        let rec_query = format!("SELECT * FROM `{}` LIMIT 100", table_name);
        if let Ok(records_rows) = sqlx::query(&rec_query).fetch_all(&state.db).await {
            if !records_rows.is_empty() {
                // ヘッダー（カラム名）の取得
                headers = records_rows[0]
                    .columns()
                    .iter()
                    .map(|c| c.name().to_string())
                    .collect::<Vec<_>>();

                // レコード値の文字列化
                for row in records_rows {
                    let mut row_data = Vec::new();
                    for i in 0..row.len() {
                        let val_str = if let Ok(s) = row.try_get::<String, _>(i) {
                            s
                        } else if let Ok(n) = row.try_get::<i64, _>(i) {
                            n.to_string()
                        } else if let Ok(n) = row.try_get::<i32, _>(i) {
                            n.to_string()
                        } else if let Ok(d) = row.try_get::<chrono::NaiveDateTime, _>(i) {
                            d.to_string()
                        } else if let Ok(d) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(i) {
                            d.to_string()
                        } else if let Ok(v) = row.try_get::<serde_json::Value, _>(i) {
                            v.to_string()
                        } else if let Ok(b) = row.try_get::<bool, _>(i) {
                            b.to_string()
                        } else if let Ok(f) = row.try_get::<f64, _>(i) {
                            f.to_string()
                        } else {
                            if row.try_get::<Option<serde_json::Value>, _>(i).ok().flatten().is_none() {
                                "NULL".to_string()
                            } else {
                                "[Unsupported Type]".to_string()
                            }
                        };
                        row_data.push(val_str);
                    }
                    records.push(row_data);
                }
            }
        }
    }

    DatabaseTemplate {
        tables,
        selected_table,
        columns,
        headers,
        records,
    }
}

/// sqlx の MySQL レコードから文字列値を取得するための堅牢な二重フォールバックヘルパー
fn get_string_value(row: &sqlx::mysql::MySqlRow, col_name: &str, index: usize) -> String {
    // 1. カラム名での String 取得を試行
    if let Ok(s) = row.try_get::<String, _>(col_name) {
        return s;
    }
    // 2. インデックスでの String 取得を試行
    if let Ok(s) = row.try_get::<String, _>(index) {
        return s;
    }
    // 3. カラム名での Vec<u8> 取得 ＆ デコードを試行 (CollationによるVARBINARY型へのフォールバック対応)
    if let Ok(bytes) = row.try_get::<Vec<u8>, _>(col_name) {
        return String::from_utf8_lossy(&bytes).into_owned();
    }
    // 4. インデックスでの Vec<u8> 取得 ＆ デコードを試行
    if let Ok(bytes) = row.try_get::<Vec<u8>, _>(index) {
        return String::from_utf8_lossy(&bytes).into_owned();
    }
    String::new()
}

/// sqlx の MySQL レコードからオプショナルな文字列値を取得するヘルパー
fn get_option_string_value(row: &sqlx::mysql::MySqlRow, col_name: &str, index: usize) -> Option<String> {
    let s = get_string_value(row, col_name, index);
    if s.is_empty() || s == "NULL" {
        None
    } else {
        Some(s)
    }
}
