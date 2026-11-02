use super::*;

const NUMBERS: [(&str, &str, &str); 6] = [
    ("platinum", "", "Platinum"),
    ("quantity", "", "Quantity"),
    ("rank", "max_rank", "Rank"),
    ("charges", "max_charges", "Charges"),
    ("amberStars", "max_amber_stars", "Amber stars"),
    ("cyanStars", "max_cyan_stars", "Cyan stars"),
];

pub(super) fn fields(item: &Value, order: &Value) -> Vec<(&'static str, &'static str, u32, u32)> {
    let mut fields = NUMBERS
        .into_iter()
        .filter_map(|(key, maximum, label)| {
            let max = number(item, maximum).max(number(order, key)) as u32;
            if maximum.is_empty() {
                Some((key, label, 1, 1_000_000))
            } else if max > 0 || order.get(key).is_some() {
                Some((key, label, 0, max))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if item["bulk_tradable"] == true || number(order, "perTrade") > 1.0 {
        fields.push(("perTrade", "Per trade", 1, 6));
    }
    fields
}

pub(super) fn draft(order: &Value) -> Value {
    let mut draft = order.clone();
    for (key, _, _) in NUMBERS {
        draft[key] = json!(number(order, key).to_string());
    }
    draft["perTrade"] = json!(number(order, "perTrade").max(1.0).to_string());
    json!({"id":order["id"],"title":"Edit order","edit":true,"order":order,"draft":draft})
}

pub(super) fn patch(dialog: &Value, item: &Value) -> Result<Value, String> {
    let draft = &dialog["draft"];
    let mut patch = json!({"visible":draft["visible"]==true});
    for (key, label, min, max) in fields(item, &dialog["order"]) {
        let value =
            bounded_value(text(draft, key), min, max).map_err(|e| format!("{label}: {e}"))?;
        patch[key] = json!(value);
    }
    if let Some(per) = patch["perTrade"].as_u64() {
        let quantity = patch["quantity"].as_u64().unwrap_or(per);
        patch["quantity"] = json!((quantity / per * per).max(per));
    }
    if !text(draft, "subtype").is_empty() {
        patch["subtype"] = draft["subtype"].clone();
    }
    Ok(patch)
}

pub(super) fn height(dialog: &Value, item: &Value) -> f32 {
    156.0
        + fields(item, &dialog["order"]).len().div_ceil(2) as f32 * 65.0
        + if subtypes(dialog, item).is_empty() {
            0.0
        } else {
            44.0
        }
}

pub(super) fn subtypes(dialog: &Value, item: &Value) -> Vec<String> {
    let mut values = entries(&item["subtypes"])
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let value = text(&dialog["order"], "subtype");
    if !value.is_empty() && !values.iter().any(|s| s == value) {
        values.push(value.into());
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_keep_variants_and_normalize_trade_packs() {
        let order = json!({"id":"one","platinum":15,"quantity":7,"visible":false,"rank":0,"charges":3,"subtype":"intact","perTrade":3});
        let item = json!({"max_rank":10,"max_charges":5,"bulk_tradable":true});
        let mut dialog = draft(&order);
        dialog["draft"]["rank"] = json!("5");
        assert_eq!(
            patch(&dialog, &item).unwrap(),
            json!({"platinum":15,"quantity":6,"visible":false,"rank":5,"charges":3,"subtype":"intact","perTrade":3})
        );
        dialog["draft"]["rank"] = json!("11");
        assert!(patch(&dialog, &item).is_err());
    }
}
