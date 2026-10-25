use super::*;

const NUMBERS: [(&str, &str, &str); 6] = [
    ("platinum", "", "Platinum"),
    ("quantity", "", "Quantity"),
    ("rank", "max_rank", "Rank"),
    ("charges", "max_charges", "Charges"),
    ("amberStars", "max_amber_stars", "Amber stars"),
    ("cyanStars", "max_cyan_stars", "Cyan stars"),
];

fn fields(item: &Value, order: &Value) -> Vec<(&'static str, &'static str, u32, u32)> {
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

fn subtypes(dialog: &Value, item: &Value) -> Vec<String> {
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

pub(super) fn render(ui: &mut Ui, dialog: &Value, item: &Value, x: f32, y: f32) {
    let fields = fields(item, &dialog["order"]);
    for (i, (key, label, min, max)) in fields.iter().enumerate() {
        let x = x + 16.0 + (i % 2) as f32 * 158.0;
        let y = y + 48.0 + (i / 2) as f32 * 65.0;
        ui.text(ui.rect(x, y, 136.0, 22.0), *label, 13.0, MUTED);
        ui.number_field(
            &format!("market:edit:{key}"),
            ui.rect(x, y + 23.0, 136.0, 34.0),
            text(&dialog["draft"], key),
            label,
            *min,
            *max,
        );
    }
    let mut y = y + 48.0 + fields.len().div_ceil(2) as f32 * 65.0;
    let subtypes = subtypes(dialog, item);
    if !subtypes.is_empty() {
        ui.choice(
            "market:edit:subtype",
            ui.rect(x + 16.0, y, 298.0, 32.0),
            text(&dialog["draft"], "subtype"),
            &subtypes
                .iter()
                .map(|s| (s.as_str(), s.as_str()))
                .collect::<Vec<_>>(),
        );
        y += 44.0;
    }
    ui.toggle(
        "market:edit:visible",
        ui.rect(x + 16.0, y, 298.0, 28.0),
        "Public",
        dialog["draft"]["visible"] == true,
    );
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
