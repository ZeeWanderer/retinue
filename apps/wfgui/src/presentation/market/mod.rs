use super::*;
use controls::{BAD, GOOD, MUTED, TEXT};
mod editor;
pub(super) mod rail;

const VARIANTS: [(&str, &str, &str); 3] = [
    ("charges", "max_charges", "Charges"),
    ("amberStars", "max_amber_stars", "Amber stars"),
    ("cyanStars", "max_cyan_stars", "Cyan stars"),
];
fn bounded_value(value: &str, min: u32, max: u32) -> Result<u32, String> {
    value
        .parse::<u32>()
        .ok()
        .filter(|v| (*v >= min) && (*v <= max))
        .ok_or_else(|| format!("Enter a whole number from {min} to {max}"))
}

pub struct Market {
    pub account: Value,
    pub matches: Value,
    pub quote: Value,
    pub description: Value,
    pub catalog: HashMap<String, Value>,
    pub dialog: Option<Value>,
    pub scroll: Scroll,
    pub offset: u32,
    search: String,
    query: String,
    item: String,
    side: String,
    email: String,
    password: String,
    price: String,
    price_edited: bool,
    quantity: String,
    rank: String,
    variants: HashMap<String, String>,
    per_trade: String,
    subtype: String,
    category: String,
    order_side: String,
    visibility: String,
    sort: String,
    descending: bool,
    stock: String,
    comparisons: HashMap<String, Value>,
    pub error: Option<String>,
    account_error: Option<String>,
    selected: HashSet<String>,
    filtered: Vec<String>,
    orders_revision: u64,
    suggestions: bool,
    suggestion: Option<usize>,
    search_at: Option<Instant>,
    pub last_quote: Instant,
}
impl Default for Market {
    fn default() -> Self {
        Self {
            account: Value::Null,
            matches: Value::Null,
            quote: Value::Null,
            description: Value::Null,
            catalog: HashMap::new(),
            dialog: None,
            scroll: Default::default(),
            offset: 0,
            search: String::new(),
            query: String::new(),
            item: String::new(),
            side: "sell".into(),
            email: String::new(),
            password: String::new(),
            price: "1".into(),
            price_edited: false,
            quantity: "1".into(),
            rank: "0".into(),
            variants: HashMap::new(),
            per_trade: "1".into(),
            subtype: String::new(),
            category: "all".into(),
            order_side: "all".into(),
            visibility: "all".into(),
            sort: "name".into(),
            descending: false,
            stock: "all".into(),
            comparisons: HashMap::new(),
            error: None,
            account_error: None,
            selected: HashSet::new(),
            filtered: Vec::new(),
            orders_revision: 0,
            suggestions: false,
            suggestion: None,
            search_at: None,
            last_quote: Instant::now(),
        }
    }
}
impl Market {
    pub fn quote_deadline(&self) -> Option<Instant> {
        (!self.item.is_empty()).then_some(self.last_quote + Duration::from_secs(60))
    }
    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        self.scroll
            .deadline(now)
            .into_iter()
            .chain(self.search_at)
            .min()
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        let next = self.scroll.offset(now).round() as u32;
        let changed = next != self.offset;
        self.offset = next;
        changed
    }
    fn item(&self) -> &Value {
        self.catalog.get(&self.item).unwrap_or(&self.description)
    }
    fn suggestions(&self) -> &[Value] {
        entries(&self.matches["resolutions"])
            .first()
            .map(|v| entries(&v["matches"]))
            .unwrap_or(&[])
    }
}
fn item_id(row: &Value) -> &str {
    text(row, "itemId")
}
fn order_name<'a>(order: &'a Value, market: &'a Market) -> &'a str {
    market
        .catalog
        .get(item_id(order))
        .map(|v| text(v, "name"))
        .filter(|s| !s.is_empty())
        .unwrap_or("Loading item...")
}

fn category(item: &Value) -> &str {
    for key in ["group", "category"] {
        if let Some(value) = item[key].as_str() {
            return value;
        }
    }
    let tags = entries(&item["tags"]);
    for (tag, category) in [
        ("relic", "relics"),
        ("arcane", "arcanes"),
        ("mod", "mods"),
        ("set", "sets"),
    ] {
        if tags.iter().any(|t| t == tag) {
            return category;
        }
    }
    let name = text(item, "name").to_lowercase();
    if [
        " blueprint",
        " chassis",
        " neuroptics",
        " systems",
        " barrel",
        " receiver",
        " blade",
        " handle",
    ]
    .iter()
    .any(|suffix| name.ends_with(suffix))
    {
        "parts"
    } else {
        "misc"
    }
}

impl Pages {
    pub(super) fn capture_market(&mut self, view: &str, select: &str) -> Result<(), String> {
        if view == "rail-search" {
            self.rail_tab = true;
            self.market_reveal = true;
            return Ok(());
        }
        if view == "login" {
            self.market.account = json!({"authenticated":false});
            return Ok(());
        }
        if !matches!(view, "edit-order" | "delete-order") {
            return Err(format!("Unsupported Market capture view: {view}"));
        }
        let order = entries(&self.market.account["orders"])
            .iter()
            .find(|order| {
                select.is_empty()
                    || text(order, "id") == select
                    || order_name(order, &self.market).eq_ignore_ascii_case(select)
            })
            .ok_or_else(|| format!("Market order not found: {select}"))?;
        let action = view.trim_end_matches("-order");
        self.market_signal(Signal {
            key: format!("market:order:{}:{action}", text(order, "id")),
            value: None,
        });
        Ok(())
    }

    fn market_owned(&self, order: &Value) -> Option<f64> {
        if !self.inventory.ready || !self.market.catalog.contains_key(item_id(order)) {
            return None;
        }
        let name = order_name(order, &self.market);
        Some(
            self.inventory
                .items
                .iter()
                .filter(|row| {
                    row.market_name.as_deref() == Some(name)
                        && order["rank"].as_u64().is_none_or(|rank| row.rank == rank)
                })
                .map(|row| row.quantity as f64)
                .sum(),
        )
    }
    pub(super) fn market_error(&mut self, key: &str, error: String) {
        if key == "market" {
            self.market.account_error = Some(error.clone());
        }
        if let Some(id) = key.strip_prefix("market:comparison:") {
            self.market.comparisons.insert(id.into(), Value::Null);
        }
        self.market.error = Some(error);
    }
    pub(crate) fn open_market(&mut self, item: String, side: &str) {
        if item.is_empty() {
            return;
        }
        self.market.item = item.clone();
        self.market.side = side.into();
        self.market.query = item.replace('_', " ");
        self.market.suggestions = false;
        self.market.suggestion = None;
        self.market.search_at = None;
        self.pending.remove("market:resolve");
        self.requests
            .retain(|request| request.key != "market:resolve");
        self.market.quote = Value::Null;
        self.market.description = Value::Null;
        self.market.rank = "0".into();
        self.market.variants.clear();
        self.market.per_trade = "1".into();
        self.market.subtype.clear();
        self.market.error = None;
        self.market.price_edited = false;
        self.rail_scroll.to(0.0, false, Instant::now());
        self.rail_tab = true;
        self.market_reveal = true;
        self.queue(
            "market:describe",
            json!({"op":"market_describe","items":[item]}),
        );
        self.market_quote(false);
        self.queue("market", json!({"op":"market_account"}));
        self.dirty = true;
    }
    pub(super) fn market_reply(&mut self, key: &str, data: Value) {
        if matches!(key, "market" | "market:describe") || key.starts_with("market:catalog:") {
            self.market.orders_revision = self.market.orders_revision.wrapping_add(1);
        }
        if !key.starts_with("market:comparison:") && !key.starts_with("market:catalog:") {
            self.market.error = None;
        }
        match key {
            "market" => {
                self.market.account = data;
                self.market.account_error = None;
                self.market.selected.retain(|id| {
                    entries(&self.market.account["orders"])
                        .iter()
                        .any(|o| text(o, "id") == id)
                });
                let ids = entries(&self.market.account["orders"])
                    .iter()
                    .map(|o| item_id(o).to_owned())
                    .filter(|id| !id.is_empty() && !self.market.catalog.contains_key(id))
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                for chunk in ids.chunks(100) {
                    self.queue(
                        &format!("market:catalog:{}", chunk[0]),
                        json!({"op":"market_describe","items":chunk}),
                    );
                }
                if self.market.account["authenticated"] == true && !self.inventory.ready {
                    self.queue("inventory", json!({"op":"inventory_view"}));
                }
            }
            "market:describe" => {
                self.market.description = entries(&data["items"])
                    .first()
                    .cloned()
                    .unwrap_or(Value::Null);
                if let Some(item) = entries(&data["items"]).first() {
                    self.market
                        .catalog
                        .insert(self.market.item.clone(), item.clone());
                    self.market.query = text(item, "name").into();
                    let subtype = entries(&item["subtypes"])
                        .first()
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if self.market.subtype.is_empty() {
                        self.market.subtype = subtype.into();
                    }
                    self.market_quote(false);
                }
            }
            "market:resolve" => {
                self.market.matches = data;
                self.market.suggestion = None;
            }
            "market:quote" => {
                let quote = entries(&data["quotes"])
                    .first()
                    .map(|q| q["quote"].clone())
                    .unwrap_or_else(|| data["quote"].clone());
                if !self.market.price_edited
                    && let Some(price) = quote[if self.market.side == "sell" {
                        "lowest_sell"
                    } else {
                        "highest_buy"
                    }]
                    .as_i64()
                {
                    self.market.price = price.max(1).to_string();
                }
                self.market.quote = quote;
                self.market.last_quote = Instant::now();
            }
            _ if key.starts_with("market:comparison:") => {
                let quote = entries(&data["quotes"])
                    .first()
                    .map(|q| q["quote"].clone())
                    .unwrap_or_else(|| data["quote"].clone());
                self.market
                    .comparisons
                    .insert(key.trim_start_matches("market:comparison:").into(), quote);
            }
            _ if key.starts_with("market:catalog:") => {
                for item in entries(&data["items"]) {
                    self.market
                        .catalog
                        .insert(text(item, "id").into(), item.clone());
                }
            }
            _ if key.starts_with("market:write") => {
                self.market.dialog = None;
                self.market.password.clear();
                self.ui.forget("market:password");
                self.queue("market", json!({"op":"market_account"}));
            }
            _ => (),
        }
    }
    fn market_quote(&mut self, refresh: bool) {
        if self.market.item.is_empty() {
            return;
        }
        let Ok(filters) = self.market_filters() else {
            return;
        };
        let request = if filters.as_object().is_some_and(|f| !f.is_empty()) {
            json!({"op":"market_quote_variant","item":self.market.item,"filters":filters,"refresh":refresh})
        } else {
            json!({"op":"market_quote","items":[self.market.item],"ttl":60,"refresh":refresh})
        };
        self.queue("market:quote", request);
    }
    fn market_filters(&self) -> Result<Value, String> {
        let mut result = json!({});
        let item = self.market.item();
        if number(item, "max_rank") > 0.0 {
            result["rank"] = json!(bounded_value(
                &self.market.rank,
                0,
                number(item, "max_rank") as u32
            )?);
        }
        for (key, maximum, _) in VARIANTS {
            let max = number(item, maximum) as u32;
            if max > 0 {
                result[key] = json!(bounded_value(
                    self.market
                        .variants
                        .get(key)
                        .map(String::as_str)
                        .unwrap_or("0"),
                    0,
                    max
                )?);
            }
        }
        if !self.market.subtype.is_empty() {
            result["subtype"] = json!(self.market.subtype);
        }
        Ok(result)
    }
    pub(super) fn market_tick(&mut self, now: Instant) {
        if self.market.search_at.is_some_and(|t| now >= t) {
            self.market.search_at = None;
            if self.market.query.trim().chars().count() >= 2 {
                self.resolve_market_query();
            }
        }
        if self.rail_tab
            && !self.market.item.is_empty()
            && now.saturating_duration_since(self.market.last_quote) >= Duration::from_secs(60)
        {
            self.market.last_quote = now;
            self.market_quote(false);
        }
    }
    fn resolve_market_query(&mut self) {
        self.market.search_at = None;
        let query = self.market.query.trim();
        if !query.is_empty() {
            self.market.suggestions = true;
            self.queue(
                "market:resolve",
                json!({"op":"market_resolve","labels":[query],"limit":8}),
            );
        }
    }
    fn submit_market_search(&mut self) {
        if let Some(item) = self
            .market
            .suggestion
            .filter(|_| self.market.suggestions)
            .and_then(|index| self.market.suggestions().get(index))
        {
            self.open_market(text(item, "slug").into(), "sell");
        } else {
            self.resolve_market_query();
        }
    }
    pub(super) fn market_search_key(
        &mut self,
        key: retinue_ui::desktop::winit::keyboard::Key<&str>,
    ) -> bool {
        use retinue_ui::{desktop::winit::keyboard::Key, desktop::winit::keyboard::NamedKey};
        if self.rail.focus.as_deref() != Some("market:query") {
            return false;
        }
        if !self.market.suggestions && key != Key::Named(NamedKey::Enter) {
            return false;
        }
        let count = self.market.suggestions().len();
        match key {
            Key::Named(NamedKey::ArrowDown) => {
                self.market.suggestion = (count > 0)
                    .then(|| self.market.suggestion.map_or(0, |i| (i + 1).min(count - 1)))
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.market.suggestion = (count > 0).then(|| {
                    self.market
                        .suggestion
                        .map_or(count - 1, |i| i.saturating_sub(1))
                })
            }
            Key::Named(NamedKey::Enter) => self.submit_market_search(),
            Key::Named(NamedKey::Escape) => self.market.suggestions = false,
            _ => return false,
        }
        self.dirty = true;
        self.layout();
        true
    }
    pub(super) fn market_signal(&mut self, signal: Signal) {
        let value = signal.value.unwrap_or_default();
        let key = signal.key.as_str();
        let writing = matches!(
            key,
            "market:login"
                | "market:password.submit"
                | "market:logout"
                | "market:presence"
                | "market:post"
                | "market:confirm"
                | "market:bulk"
        ) || key.starts_with("market:order:")
            && !key.ends_with(":select")
            && !key.ends_with(":list");
        if writing
            && self
                .pending
                .keys()
                .any(|k| k.starts_with("market:write") || k == "market")
        {
            return;
        }
        match key{
            "market:scroll"=>{if let Ok(y)=value.parse(){self.market.scroll.to(y,false,Instant::now());self.market.offset=y.max(0.0) as u32;}},
            "market:email"=>self.market.email=value,
            "market:password"=>self.market.password=value,
            "market:login"|"market:password.submit"=>self.queue("market:write:login",json!({"op":"market_login","email":self.market.email,"password":self.market.password})),
            "market:logout"=>self.queue("market:write:logout",json!({"op":"market_logout"})),
            "market:presence"=>self.queue("market:write:presence",json!({"op":"market_presence_set","mode":value})),
            "market:refresh"=>{self.market.comparisons.clear();self.queue("market",json!({"op":"market_account"}));},
            "market:browse"=>self.rail_tab=true,
            "market:search"=>self.market.search=value,
            "market:side"=>self.market.order_side=value,
            "market:visibility"=>self.market.visibility=value,
            "market:category"=>self.market.category=value,
            "market:sort"=>self.market.sort=value,
            "market:stock"=>self.market.stock=value,
            "market:direction"=>self.market.descending=!self.market.descending,
            "market:query"=>{
                self.market.query=value;
                self.market.matches=Value::Null;
                self.market.suggestion=None;
                self.market.suggestions=self.market.query.trim().chars().count()>=2;
                self.market.search_at=self.market.suggestions.then(||Instant::now()+Duration::from_millis(180));
                self.pending.remove("market:resolve");
                self.requests.retain(|r|r.key!="market:resolve");
            },
            "market:query.submit"=>self.submit_market_search(),
            "market:rank"=>{self.market.rank=value;self.market_quote(false);},
            "market:subtype"=>{self.market.subtype=value;self.market_quote(false);},
            "market:price"=>{self.market.price=value;self.market.price_edited=true;},
            "market:quantity"=>self.market.quantity=value,
            "market:per-trade"=>self.market.per_trade=value,
            "market:quote-refresh"=>self.market_quote(true),
            "market:website"=>{let slug=text(self.market.item(),"slug");if !slug.is_empty(){self.external=Some(format!("https://warframe.market/items/{slug}"));}},
            "market:post"=>{
                let Some((price,quantity))=positive_values(&self.market.price,&self.market.quantity)else{self.error=Some("Enter a positive whole price and quantity".into());return;};
                if self.market.account["authenticated"]!=true||text(self.market.item(),"id").is_empty(){return;}
                let mut order=match self.market_filters(){Ok(value)=>value,Err(error)=>{self.error=Some(error);return;}};
                order["itemId"]=json!(text(self.market.item(),"id"));order["type"]=json!(self.market.side);order["platinum"]=json!(price);order["quantity"]=json!(quantity);order["visible"]=json!(true);
                if self.market.item()["bulk_tradable"]==true{
                    let per=match bounded_value(&self.market.per_trade,1,6){Ok(v)=>v,Err(e)=>{self.error=Some(e);return;}};
                    order["perTrade"]=json!(per);order["quantity"]=json!((quantity/per*per).max(per));
                }
                self.queue("market:write:create",json!({"op":"market_order_create","order":order}));
            }
            "market:bulk"=>match value.as_str(){
                "public"|"hidden"=>self.queue("market:write:visibility",json!({"op":"market_orders_visibility","visible":value=="public"})),
                "delete"|"filtered"=>{
                    let ids=if value=="delete"{self.market.selected.iter().cloned().collect::<Vec<_>>()}else{self.market.filtered.clone()};
                    if !ids.is_empty(){self.market.dialog=Some(json!({"title":"Delete orders?","message":format!("{} orders will be removed.",ids.len()),"operations":ids.into_iter().map(|id|json!({"op":"market_order_delete","order_id":id})).collect::<Vec<_>>()}));}
                },
                "reconcile"=>self.reconcile_market(),
                _=>()
            },
            "market:cancel"=>self.market.dialog=None,
            "market:confirm"=>{
                if let Some(dialog)=self.market.dialog.clone(){
                    if let Some(operations)=dialog["operations"].as_array(){for (i,op) in operations.iter().enumerate(){self.queue(&format!("market:write:batch:{i}"),op.clone());}}
                    else if dialog["edit"]==true{
                        let item=self.market.catalog.get(item_id(&dialog["order"])).unwrap_or(&Value::Null);
                        match editor::patch(&dialog,item){
                            Ok(patch)=>self.queue("market:write:edit",json!({"op":"market_order_update","order_id":dialog["id"],"patch":patch})),
                            Err(error)=>self.market.error=Some(error),
                        }
                    }else{self.queue("market:write:delete",json!({"op":"market_order_delete","order_id":dialog["id"]}));}
                }
            }
            _ if key.starts_with("market:edit:")=>{
                if let Some(dialog)=&mut self.market.dialog {
                    let field=key.trim_start_matches("market:edit:");
                    dialog["draft"][field]=if field=="visible"{json!(dialog["draft"][field]!=true)}else{json!(value)};
                }
            }
            _ if key.starts_with("market:variant:")=>{self.market.variants.insert(key.trim_start_matches("market:variant:").into(),value);self.market_quote(false);},
            _ if key.starts_with("market:match:")=>{
                if let Some(i)=key.rsplit(':').next().and_then(|v|v.parse::<usize>().ok())&&let Some(item)=self.market.suggestions().get(i){self.open_market(text(item,"slug").into(),"sell");}
            }
            _ if key.starts_with("market:mode:")=>{self.market.side=key.rsplit(':').next().unwrap_or("sell").into();self.rail_scroll.to(0.0,false,Instant::now());}
            _ if key.starts_with("market:whisper:")=>{
                if let Some(i)=key.rsplit(':').next().and_then(|v|v.parse::<usize>().ok())&&let Some(order)=entries(&self.market.quote[if self.market.side=="sell"{"sell_orders"}else{"buy_orders"}]).get(i){
                    if let Some(whisper)=whisper(order,text(self.market.item(),"name"),&self.market.side){
                        self.rail.clipboard=Some(("market:whisper".into(),Instant::now(),retinue_ui::desktop::clipboard::Operation::Write(whisper)));
                    }else{self.market.error=Some("Listing has no player name".into());}
                }
            }
            _ if key.starts_with("market:order:")=>{
                let Some((id,action))=key.trim_start_matches("market:order:").rsplit_once(':')else{return};
                let Some(order)=entries(&self.market.account["orders"]).iter().find(|o|text(o,"id")==id).cloned()else{return};
                match action{
                    "select"=>{if !self.market.selected.remove(id){self.market.selected.insert(id.into());}},
                    "visibility"=>self.queue("market:write:visibility",json!({"op":"market_order_update","order_id":id,"patch":{"visible":order["visible"]!=true}})),
                    "add"=>self.queue("market:write:quantity",json!({"op":"market_order_update","order_id":id,"patch":{"quantity":number(&order,"quantity") as u64+number(&order,"perTrade").max(1.0) as u64}})),
                    "close"=>self.queue("market:write:close",json!({"op":"market_order_close","order_id":id,"quantity":number(&order,"perTrade").max(1.0) as u64})),
                    "list"=>{
                        self.open_market(item_id(&order).into(),"sell");
                        self.market.rank=number(&order,"rank").to_string();
                        self.market.subtype=text(&order,"subtype").into();
                        self.market.per_trade=number(&order,"perTrade").max(1.0).to_string();
                        for (key,_,_) in VARIANTS {if let Some(value)=order.get(key){self.market.variants.insert(key.into(),value.to_string());}}
                        self.market_quote(false);
                    },
                    "edit"=>{self.market.error=None;self.market.dialog=Some(editor::draft(&order));},
                    "delete"=>self.market.dialog=Some(json!({"id":id,"title":"Delete order?","message":order_name(&order,&self.market)})),_=>()
                }
            }
            _=>(),
        }
    }
    fn reconcile_market(&mut self) {
        let mut operations = vec![];
        for order in entries(&self.market.account["orders"])
            .iter()
            .filter(|o| o["type"] == "sell")
        {
            let Some(owned) = self.market_owned(order) else {
                self.market.error =
                    Some("Wait for inventory and item details before reconciling orders".into());
                return;
            };
            // Non-rank variants are not distinguished in the inventory projection.
            if ["charges", "amberStars", "cyanStars", "subtype"]
                .iter()
                .any(|key| order.get(key).is_some_and(|v| !v.is_null()))
            {
                self.market.error =
                    Some("Variant quantities cannot be reconciled automatically".into());
                return;
            }
            let per = number(order, "perTrade").max(1.0) as u64;
            let target = owned as u64 / per * per;
            if target < number(order, "quantity") as u64 {
                operations.push(if target<per {json!({"op":"market_order_delete","order_id":order["id"]})}
                    else{json!({"op":"market_order_update","order_id":order["id"],"patch":{"quantity":target}})});
            }
        }
        if operations.is_empty() {
            self.market.error = Some("Sell quantities already match inventory".into());
            return;
        }
        self.market.dialog = Some(
            json!({"title":"Reconcile sell orders?","message":format!("Adjust or remove {} orders to match inventory.",operations.len()),"operations":operations}),
        );
    }
}
fn positive_values(price: &str, quantity: &str) -> Option<(u32, u32)> {
    let p = price.parse().ok()?;
    let q = quantity.parse().ok()?;
    (p > 0 && q > 0).then_some((p, q))
}
fn whisper(order: &Value, item: &str, side: &str) -> Option<String> {
    let user = &order["user"];
    let name = if text(user, "ingameName").is_empty() {
        text(user, "ingame_name")
    } else {
        text(user, "ingameName")
    };
    if name.is_empty() {
        return None;
    }
    let mut details = vec![];
    if !text(order, "subtype").is_empty() {
        details.push(text(order, "subtype").to_owned());
    }
    for (key, label) in [
        ("rank", "Rank"),
        ("charges", "Charges"),
        ("amberStars", "Amber stars"),
        ("cyanStars", "Cyan stars"),
    ] {
        if let Some(value) = order[key].as_u64() {
            details.push(format!("{label} {value}"));
        }
    }
    let details = if details.is_empty() {
        String::new()
    } else {
        format!(" ({})", details.join(", "))
    };
    let per = number(order, "perTrade").max(1.0) as u64;
    let pack = if per > 1 {
        format!(" x{per}")
    } else {
        String::new()
    };
    let action = if side == "sell" { "buy" } else { "sell" };
    Some(format!(
        "/w {name} Hi! I want to {action}:{pack} \"{item}{details}\" for {:.0} platinum. (warframe.market)",
        number(order, "platinum")
    ))
}
fn market_icon(name: &str) -> &'static [u8] {
    macro_rules! icon {
        ($n:literal) => {
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/resources/market/wfgui-",
                $n,
                ".png"
            ))
        };
    }
    match name {
        "eye" => icon!("eye"),
        "boxes" => icon!("boxes"),
        "eye-off" => icon!("eye-off"),
        "search" => icon!("search"),
        "pencil" => icon!("pencil"),
        "plus" => icon!("plus"),
        "check" => icon!("check"),
        _ => icon!("trash"),
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;
    use retinue_ui::{desktop::winit::keyboard::Key, desktop::winit::keyboard::NamedKey};

    #[test]
    fn rail_search_debounces_two_characters_and_enter_searches_immediately() {
        let mut pages = Pages::new(Default::default()).unwrap();
        pages.rail_tab = true;
        pages.configure((1000, 900), (400, 900), 1.0);
        pages.rail.focus = Some("market:query".into());
        pages.market_signal(Signal {
            key: "market:query".into(),
            value: Some("s".into()),
        });
        pages.market_tick(Instant::now() + Duration::from_secs(1));
        assert!(!pages.pending.contains_key("market:resolve"));

        pages.market_signal(Signal {
            key: "market:query".into(),
            value: Some("sa".into()),
        });
        let due = pages.market.search_at.unwrap();
        assert!(pages.deadline().is_some_and(|deadline| deadline <= due));
        pages.market_tick(due - Duration::from_millis(1));
        assert!(!pages.pending.contains_key("market:resolve"));
        pages.market_tick(due);
        let old = pages
            .requests
            .iter()
            .find(|r| r.key == "market:resolve")
            .unwrap()
            .clone();
        assert_eq!(old.body["labels"], json!(["sa"]));

        pages.market_signal(Signal {
            key: "market:query".into(),
            value: Some("saryn".into()),
        });
        pages.update(Update::Reply {
            request: old,
            result: Ok(crate::domain::session::Reply::Json(
                json!({"resolutions":[{"matches":[{"slug":"stale"}]}]}),
            )),
        });
        assert!(pages.market.suggestions().is_empty());
        assert!(pages.market_search_key(Key::Named(NamedKey::Enter)));
        assert_eq!(pages.pending["market:resolve"]["labels"], json!(["saryn"]));
        assert!(pages.market.search_at.is_none());

        pages.market_reply(
            "market:resolve",
            json!({"resolutions":[{"matches":[{"slug":"saryn_prime"}]}]}),
        );
        assert!(pages.market_search_key(Key::Named(NamedKey::ArrowDown)));
        assert!(pages.market_search_key(Key::Named(NamedKey::Escape)));
        assert!(pages.market_search_key(Key::Named(NamedKey::Enter)));
        assert!(pages.market.item.is_empty());
        pages.market_reply(
            "market:resolve",
            json!({"resolutions":[{"matches":[{"slug":"saryn_prime"}]}]}),
        );
        assert!(pages.market_search_key(Key::Named(NamedKey::ArrowDown)));
        assert!(pages.market_search_key(Key::Named(NamedKey::Enter)));
        assert_eq!(pages.market.item, "saryn_prime");

        pages.market_signal(Signal {
            key: "market:query".into(),
            value: Some(String::new()),
        });
        assert!(pages.market_search_key(Key::Named(NamedKey::Enter)));
        assert!(!pages.pending.contains_key("market:resolve"));
        assert!(pages.market.suggestions().is_empty());
    }

    #[test]
    fn market_search_layouts_match_their_surrounding_controls_at_each_scale() {
        for scale in [1.0, 1.25, 2.0] {
            let mut pages = Pages::new(Default::default()).unwrap();
            pages.fixture();
            pages.set_page(Page::Market);
            pages.rail_tab = true;
            pages.market.query.clear();
            pages.configure(
                ((1000.0 * scale) as u32, (900.0 * scale) as u32),
                ((400.0 * scale) as u32, (900.0 * scale) as u32),
                scale,
            );
            let search = pages.rail.bounds("market:query").unwrap();
            assert_eq!(search.width, pages.rail.px(32.0));
            assert_eq!(search.width, search.height);
            assert_eq!(search.x + search.width, pages.rail.px(390.0));
            assert!(pages.rail.bounds("market:query.submit").is_none());
            let orders = pages.ui.bounds("market:search").unwrap();
            let side = pages.ui.bounds("market:side").unwrap();
            assert_eq!(orders.height, side.height);
            assert_eq!(orders.y, side.y);
            assert!((orders.x + orders.width + pages.ui.px(10.0)).abs_diff(side.x) <= 1);
            pages.market.search = "Hush".into();
            pages.dirty = true;
            pages.layout();
            assert!(pages.ui.bounds("clear:market:search").is_some());

            pages
                .capture_state("rail-search", "", "Hush", Some("Ember Prime Set"))
                .unwrap();
            pages.settle_capture();
            let expanded = pages.rail.bounds("market:query").unwrap();
            assert_eq!(expanded.x, pages.rail.px(10.0));
            assert_eq!(expanded.width, pages.rail.px(380.0));
            assert_eq!(pages.market.search, "Hush");
            assert!(pages.rail.bounds("clear:market:query").is_some());
        }
    }
    #[test]
    fn order_inputs_never_silently_become_zero() {
        assert_eq!(positive_values("0", "2"), None);
        assert_eq!(positive_values("4.5", "2"), None);
        assert_eq!(positive_values("5", "2"), Some((5, 2)));
    }
    #[test]
    fn whispers_identify_variant_and_pack() {
        let order =
            json!({"user":{"ingameName":"Trader"},"rank":5,"charges":2,"perTrade":3,"platinum":24});
        assert_eq!(
            whisper(&order, "Mod", "sell").unwrap(),
            "/w Trader Hi! I want to buy: x3 \"Mod (Rank 5, Charges 2)\" for 24 platinum. (warframe.market)"
        );
        assert!(whisper(&Value::Null, "Mod", "buy").is_none());
    }
    #[test]
    fn listing_navigation_preserves_rank_and_quotes_do_not_overwrite_edited_price() {
        let mut pages = Pages::new(Default::default()).unwrap();
        pages.fixture();
        pages.fixture = false;
        pages.market.account["orders"][0]["rank"] = json!(5);
        pages.market_signal(Signal {
            key: "market:order:example:list".into(),
            value: None,
        });
        let description = json!({"items":[{"id":"example-item","name":"Mod","max_rank":10}]});
        pages.market_reply("market:describe", description);
        assert_eq!(pages.market_filters().unwrap()["rank"], 5);
        pages.market_signal(Signal {
            key: "market:price".into(),
            value: Some("22".into()),
        });
        pages.market_reply("market:quote", json!({"quote":{"lowest_sell":5}}));
        assert_eq!(pages.market.price, "22");
    }
}

mod account;
mod card;
mod dialog;
mod grid_view;
mod market_model;
mod market_view;
#[cfg(test)]
mod tests;
mod toolbar;

use crate::{
    presentation::widgets::Fonts, presentation::widgets::fill, presentation::widgets::fitted_label,
    presentation::widgets::key, presentation::widgets::place, presentation::widgets::slot,
};
use market_model::Model;
use retinue_render::{Bounds, CacheHint, Key};
use retinue_ui::{layout::grid::UniformGrid, prelude::*, semantics::Role};

const GRID: Key = Key::new("market.grid", 0);
const SCROLL: Key = Key::new("market.scroll", 0);

#[derive(Clone, PartialEq, Default)]
struct Metrics {
    width: f32,
    height: f32,
    scale: f32,
}

impl Metrics {
    fn bounds(&self, x: f32, y: f32, w: f32, h: f32) -> Bounds {
        Bounds::new(x.max(0.0), y.max(0.0), w.max(0.0), h.max(0.0))
    }
    fn local(&self, parent: Bounds, child: Bounds) -> Bounds {
        let global = self.bounds(
            parent.x + child.x,
            parent.y + child.y,
            child.width,
            child.height,
        );
        Bounds::new(
            global.x - parent.x,
            global.y - parent.y,
            global.width,
            global.height,
        )
    }
    fn actions_y(&self) -> f32 {
        if self.width < 650.0 { 170.0 } else { 130.0 }
    }
    fn viewport(&self) -> Bounds {
        let y = self.actions_y() + 44.0;
        self.bounds(10.0, y, self.width - 20.0, self.height - y)
    }
    fn grid(&self) -> retinue_ui::grid::Grid {
        let px = |v: f32| (v.max(0.0) * self.scale).round() as u32;
        retinue_ui::grid::Grid::new(px(self.viewport().width), px(370.0), px(107.0), px(8.0))
    }
    fn uniform_grid(&self) -> UniformGrid {
        let g = self.grid();
        UniformGrid::new(
            g.columns,
            LayoutSize::new(
                (g.width as f32 / self.scale).max(1.0),
                g.height as f32 / self.scale,
            ),
            LayoutSize::new(g.gap as f32 / self.scale, g.gap as f32 / self.scale),
            Insets::default(),
        )
        .expect("market grid geometry")
    }
}

#[derive(Clone, PartialEq, Default)]
struct AccountState {
    authenticated: Option<bool>,
    busy: bool,
    name: String,
    error: Option<String>,
    account_error: Option<String>,
}

#[derive(Clone, PartialEq)]
struct Filter {
    search: String,
    side: String,
    visibility: String,
    category: String,
    sort: String,
    stock: String,
    descending: bool,
}

impl From<&Market> for Filter {
    fn from(m: &Market) -> Self {
        Self {
            search: m.search.to_lowercase(),
            side: m.order_side.clone(),
            visibility: m.visibility.clone(),
            category: m.category.clone(),
            sort: m.sort.clone(),
            stock: m.stock.clone(),
            descending: m.descending,
        }
    }
}

#[derive(PartialEq)]
struct ProjectionKey {
    market: u64,
    inventory: u64,
    inventory_ready: bool,
    authenticated: bool,
    filter: Filter,
}

struct Order {
    source: usize,
    owned: Option<f64>,
    missing: bool,
}

pub(in crate::presentation) struct MarketController {
    model: Model,
    dialog: dialog::Dialog,
    projection: Option<ProjectionKey>,
    orders: Vec<Order>,
    pub installed: bool,
}

impl MarketController {
    pub fn new() -> Self {
        Self {
            model: Model::default(),
            dialog: dialog::Dialog::new(),
            projection: None,
            orders: Vec::new(),
            installed: false,
        }
    }

    fn view(&self, ui: &ControlHost) -> Element {
        let fonts = Fonts::new(ui);
        market_view::MarketPageView {
            model: self.model.clone(),
            dialog: self.dialog.view(self.model.metrics.clone(), fonts.clone()),
            fonts,
        }
        .into_element()
    }

    fn project(&mut self, market: &mut Market, inventory: &Listing) {
        let projection = ProjectionKey {
            market: market.orders_revision,
            inventory: inventory.revision(),
            inventory_ready: inventory.ready,
            authenticated: market.account["authenticated"] == true,
            filter: Filter::from(&*market),
        };
        if self.projection.as_ref() == Some(&projection) {
            return;
        }
        let f = &projection.filter;
        let mut owned = HashMap::new();
        if inventory.ready && projection.authenticated {
            for row in &inventory.items {
                if let Some(name) = row.market_name.as_deref() {
                    *owned.entry((name, None)).or_insert(0.0) += row.quantity as f64;
                    *owned.entry((name, Some(row.rank))).or_insert(0.0) += row.quantity as f64;
                }
            }
        }
        let source = entries(&market.account["orders"]);
        self.orders = source
            .iter()
            .enumerate()
            .filter_map(|(index, order)| {
                if !projection.authenticated {
                    return None;
                }
                let description = market.catalog.get(item_id(order));
                let name = order_name(order, market);
                let owned = (inventory.ready && description.is_some()).then(|| {
                    owned
                        .get(&(name, order["rank"].as_u64()))
                        .copied()
                        .unwrap_or(0.0)
                });
                let missing =
                    order["type"] == "sell" && owned.is_some_and(|n| n < number(order, "quantity"));
                ((f.search.is_empty() || name.to_lowercase().contains(&f.search))
                    && (f.stock == "all" || missing == (f.stock == "missing"))
                    && (f.side == "all" || text(order, "type") == f.side)
                    && match f.visibility.as_str() {
                        "public" => order["visible"] == true,
                        "hidden" => order["visible"] != true,
                        _ => true,
                    }
                    && (f.category == "all"
                        || description.is_some_and(|d| category(d) == f.category)))
                .then_some(Order {
                    source: index,
                    owned,
                    missing,
                })
            })
            .collect();
        self.orders.sort_by(|a, b| {
            let (a, b) = (&source[a.source], &source[b.source]);
            let order = match f.sort.as_str() {
                "price" => number(a, "platinum").total_cmp(&number(b, "platinum")),
                "quantity" => number(a, "quantity").total_cmp(&number(b, "quantity")),
                _ => order_name(a, market).cmp(order_name(b, market)),
            };
            if f.descending { order.reverse() } else { order }
        });
        market.filtered = self
            .orders
            .iter()
            .map(|o| text(&source[o.source], "id").to_owned())
            .collect();
        self.model.order.set(
            KeyedOrder::new(market.filtered.iter().map(|id| key("market.order", id)))
                .expect("unique market orders"),
        );
        self.projection = Some(projection);
    }

    fn sync(
        &mut self,
        ui: &ControlHost,
        market: &mut Market,
        inventory: &Listing,
        state: AccountState,
        now: Instant,
    ) {
        let m = Metrics {
            width: ui.size.0 as f32 / ui.scale,
            height: ui.size.1 as f32 / ui.scale,
            scale: ui.scale,
        };
        self.model.metrics.set(m.clone());
        self.model.account.set(state);
        self.project(market, inventory);
        let grid = m.grid();
        market.scroll.resize(
            grid.content_height(self.orders.len()) as f32,
            m.viewport().height * ui.scale,
            now,
        );
        market.offset = market.scroll.offset(now).round() as u32;
        let window = grid.visible(
            self.orders.len(),
            market.offset,
            (m.viewport().height * ui.scale).round() as u32,
            0,
        );
        let order = self.model.order.get();
        self.model
            .cells
            .replace(
                window
                    .clone()
                    .map(|i| (order.keys()[i], card::Card::new(&self.orders[i], market))),
            )
            .expect("visible market orders");
        self.model.offset.set(market.offset as f32 / ui.scale);
        self.dialog.sync(market);
    }

    fn bind(&self, ui: &mut ControlHost, market: &Market, connected: bool) {
        let state = self.model.account.get();
        if state.authenticated != Some(true) {
            account::bind(ui, market, &state, connected);
            return;
        }
        toolbar::bind(ui, market, state.busy);
        if let Some(r) = ui.view_bounds(GRID) {
            ui.region("page.grid", r);
        }
        for &id in self.model.cells.order().keys() {
            if let Some(r) = ui.view_bounds(id) {
                ui.first_region("page.card", r);
            }
            card::bind(ui, id, &self.model.cells.get(id).unwrap(), state.busy);
        }
        ui.view_control(SCROLL, |ui, r| {
            ui.scrollbar(
                "market:scroll",
                r,
                market.scroll.maximum(),
                market.offset as f32,
            )
        });
        self.dialog.bind(ui, market, state.busy);
    }
}

impl Pages {
    pub(in crate::presentation) fn layout_market(&mut self) {
        let state = AccountState {
            authenticated: self.market.account["authenticated"].as_bool(),
            busy: self.pending.keys().any(|k| k.starts_with("market:write")),
            name: text(&self.market.account["profile"], "ingameName").into(),
            error: self.market.error.as_ref().or(self.error.as_ref()).cloned(),
            account_error: self
                .market
                .account_error
                .as_ref()
                .or(self.error.as_ref())
                .filter(|_| !self.pending.contains_key("market"))
                .cloned(),
        };
        let view = &mut self.market_view;
        view.sync(
            &self.ui,
            &mut self.market,
            &self.inventory,
            state,
            Instant::now(),
        );
        if !view.installed {
            self.ui.install_view(view.view(&self.ui));
            view.installed = true;
        }
        self.ui.update_view(FrameTime::now());
        view.bind(&mut self.ui, &self.market, self.connected);
        let requests = view
            .model
            .cells
            .order()
            .keys()
            .iter()
            .filter_map(|&id| {
                let cell = view.model.cells.get(id)?;
                if self.market.comparisons.contains_key(&cell.id) {
                    return None;
                }
                let order = &entries(&self.market.account["orders"])[cell.source];
                if !self.market.catalog.contains_key(item_id(order)) {
                    return None;
                }
                let filters = order
                    .as_object()?
                    .iter()
                    .filter(|(key, _)| {
                        matches!(
                            key.as_str(),
                            "rank" | "charges" | "amberStars" | "cyanStars" | "subtype"
                        )
                    })
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<serde_json::Map<_, _>>();
                let op = if filters.is_empty() {
                    json!({"op":"market_quote","items":[item_id(order)],"ttl":60})
                } else {
                    json!({"op":"market_quote_variant","item":item_id(order),"filters":filters})
                };
                Some((format!("market:comparison:{}", cell.id), op))
            })
            .collect::<Vec<_>>();
        for (key, op) in requests {
            self.queue(&key, op);
        }
    }
}
