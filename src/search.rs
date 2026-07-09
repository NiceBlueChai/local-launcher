//! Pure search and sorting for launcher items.

use crate::model::{ItemKind, LauncherItem};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CategoryFilter {
    All,
    Favorites,
    Kind(ItemKind),
    Category(String),
}

/// Filters and sorts launcher items for the main list and future command panel.
pub fn filter_items(
    items: &[LauncherItem],
    query: &str,
    filter: &CategoryFilter,
) -> Vec<LauncherItem> {
    let query = query.trim().to_lowercase();
    let mut scored: Vec<(i32, LauncherItem)> = items
        .iter()
        .filter(|item| matches_filter(item, filter))
        .filter_map(|item| score_item(item, &query).map(|score| (score, item.clone())))
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    scored.into_iter().map(|(_, item)| item).collect()
}

fn matches_filter(item: &LauncherItem, filter: &CategoryFilter) -> bool {
    match filter {
        CategoryFilter::All => true,
        CategoryFilter::Favorites => item.favorite,
        CategoryFilter::Kind(kind) => &item.kind == kind,
        CategoryFilter::Category(category) => item.category == *category,
    }
}

fn score_item(item: &LauncherItem, query: &str) -> Option<i32> {
    let mut score = if item.favorite { 100 } else { 0 };
    if query.is_empty() {
        return Some(score);
    }

    let name = item.name.to_lowercase();
    let category = item.category.to_lowercase();
    let target = item.target.to_lowercase();
    let notes = item.notes.to_lowercase();

    if name == query {
        score += 50;
    } else if name.contains(query) {
        score += 40;
    } else if category.contains(query)
        || item
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(query))
    {
        score += 25;
    } else if target.contains(query) || notes.contains(query) {
        score += 10;
    } else {
        return None;
    }

    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, name: &str, kind: ItemKind, favorite: bool) -> LauncherItem {
        LauncherItem {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            target: format!("target-{id}"),
            arguments: String::new(),
            category: "开发".to_string(),
            tags: vec!["工具".to_string()],
            username: String::new(),
            favorite,
            notes: "备注".to_string(),
            icon_format: None,
            icon_data: None,
        }
    }

    #[test]
    fn favorite_items_sort_first() {
        let items = vec![
            item("a", "Alpha", ItemKind::Program, false),
            item("b", "Beta", ItemKind::Program, true),
        ];

        let result = filter_items(&items, "", &CategoryFilter::All);

        assert_eq!(result[0].id, "b");
    }

    #[test]
    fn filters_by_kind_and_query() {
        let items = vec![
            item("a", "Docs", ItemKind::Folder, false),
            item("b", "Docs Site", ItemKind::Url, false),
        ];

        let result = filter_items(&items, "docs", &CategoryFilter::Kind(ItemKind::Url));

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "b");
    }

    #[test]
    fn filters_by_custom_category() {
        let items = vec![
            item("a", "Docs", ItemKind::Folder, false),
            LauncherItem {
                category: "内网".to_string(),
                ..item("b", "Admin", ItemKind::IntranetUrl, false)
            },
        ];

        let result = filter_items(&items, "", &CategoryFilter::Category("内网".to_string()));

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "b");
    }
}
