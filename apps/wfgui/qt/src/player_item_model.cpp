#include "player_item_model.h"

#include <QJsonArray>
#include <QSet>
#include <QVariantList>
#include <QVariantMap>

#include <algorithm>
#include <tuple>

namespace {
QString assetId(const QJsonObject &item) {
  return item.value("asset").toObject().value("id").toString();
}

QString marketName(const QJsonObject &item) {
  const QString explicitName = item.value("market_name").toString();
  if (!explicitName.isEmpty()) {
    return explicitName;
  }
  const QString name = item.value("name").toString();
  if (item.value("group").toString() != "relics") {
    return name;
  }
  const QStringList parts = name.split(' ', Qt::SkipEmptyParts);
  return parts.size() >= 2 ? parts.at(0) + ' ' + parts.at(1) + " Relic" : name;
}

int flagRole(const QString &name) {
  static const QHash<QString, int> roles = {
      {"mastered", PlayerItemModel::MasteredRole},
      {"owned", PlayerItemModel::OwnedRole},
      {"ready", PlayerItemModel::ReadyToBuildRole},
      {"prime", PlayerItemModel::IsPrimeRole},
      {"favorite", PlayerItemModel::FavoriteRole},
      {"vaulted", PlayerItemModel::VaultedRole},
  };
  return roles.value(name, -1);
}

struct AcquisitionQuote {
  QVariant platinum;
  QString state;
};

AcquisitionQuote
acquisitionQuote(const QJsonObject &item,
                 const QHash<QString, QJsonObject> &marketQuotes,
                 const QSet<QString> &unavailableMarketItems) {
  bool hasMissingComponent = false;
  bool loading = false;
  bool requiredUnavailable = false;
  int total = 0;
  for (const QJsonValue &value : item.value("components").toArray()) {
    const QJsonObject component = value.toObject();
    const int missing = qMax(0, component.value("required").toInt() -
                                    component.value("owned").toInt());
    if (missing == 0) {
      continue;
    }
    hasMissingComponent = true;
    const bool required = component.value("market_required").toBool();
    const QString marketName = component.value("market_name").toString();
    if (marketName.isEmpty()) {
      requiredUnavailable = requiredUnavailable || required;
      continue;
    }
    const auto quote = marketQuotes.constFind(marketName);
    if (quote == marketQuotes.cend()) {
      if (unavailableMarketItems.contains(marketName)) {
        requiredUnavailable = requiredUnavailable || required;
      } else {
        loading = true;
      }
      continue;
    }
    const QJsonValue price = quote->value("lowest_sell");
    if (price.isDouble()) {
      total += missing * price.toInt();
    } else {
      requiredUnavailable = requiredUnavailable || required;
    }
  }
  if (loading) {
    return {{}, QStringLiteral("loading")};
  }
  if (requiredUnavailable) {
    return {{}, QStringLiteral("unavailable")};
  }
  return {QVariant(hasMissingComponent ? total : 0), QStringLiteral("ready")};
}
} // namespace

PlayerItemModel::PlayerItemModel(QObject *parent)
    : QAbstractListModel(parent) {}

int PlayerItemModel::rowCount(const QModelIndex &parent) const {
  return parent.isValid() ? 0 : items_.size();
}

int PlayerItemModel::ownedQuantity(const QString &name,
                                   std::optional<int> rank) const {
  const QString key = name.trimmed().toCaseFolded();
  int quantity = 0;
  for (const QJsonObject &item : items_) {
    const bool matches = marketName(item).trimmed().toCaseFolded() == key ||
                         item.value("name").toString().trimmed().toCaseFolded() ==
                             key;
    if (!matches ||
        (rank && (!item.contains("rank") ||
                  item.value("rank").toInt() != *rank))) {
      continue;
    }
    quantity += item.value("quantity").toInt();
  }
  return quantity;
}

QVariant PlayerItemModel::data(const QModelIndex &index, int role) const {
  if (!index.isValid() || index.row() < 0 || index.row() >= items_.size()) {
    return {};
  }
  const QJsonObject &item = items_.at(index.row());
  switch (role) {
  case NameRole:
  case Qt::DisplayRole:
  case Qt::ToolTipRole:
    return item.value("name").toString();
  case IdRole:
    return item.value("id").toString();
  case GroupRole:
    return item.value("group").toString();
  case CategoryRole:
    return item.value("category").toString();
  case TypeRole:
    return item.value("type").toString();
  case QuantityRole:
    return item.value("quantity").toInt();
  case DucatsRole:
    return item.value("ducats").toInt();
  case MasteredRole:
    return item.value("mastered").toBool();
  case AssetPathRole:
    return assets_.value(assetId(item)).path;
  case AssetRefRole:
    return QVariant::fromValue(assets_.value(assetId(item)));
  case OwnedRole:
    return item.value("owned").toBool();
  case PendingRole:
    return item.value("pending").toBool();
  case RankRole:
    return item.value("rank").toInt();
  case MaxRankRole:
    return item.value("max_rank").toInt();
  case PotentialXpRole:
    return item.value("potential_xp").toInt();
  case MissingPartsRole:
    return item.value("missing_parts").toInt();
  case FromRelicsRole:
    return item.value("from_relics").toBool();
  case RelicProbabilityRole:
    return item.value("relic_probability").toDouble();
  case BuyableRole:
    return item.value("buyable").toBool();
  case AcquisitionPlatinumRole:
    return acquisitionQuote(item, marketQuotes_, unavailableMarketItems_)
        .platinum;
  case AcquisitionPriceStateRole:
    return acquisitionQuote(item, marketQuotes_, unavailableMarketItems_).state;
  case HasRecipeRole:
    return item.value("has_recipe").toBool();
  case ComponentsRole: {
    const auto cached = componentCache_.constFind(index.row());
    if (cached != componentCache_.cend()) {
      return cached.value();
    }
    QVariantList result;
    for (const QJsonValue &value : item.value("components").toArray()) {
      const QJsonObject component = value.toObject();
      QVariantMap converted = component.toVariantMap();
      const wfgui::AssetRef asset = assets_.value(assetId(component));
      converted.insert("image", asset.path);
      converted.insert("assetRef", QVariant::fromValue(asset));
      result.append(converted);
    }
    componentCache_.insert(index.row(), result);
    return result;
  }
  case AssetSpecRole:
    return item.value("asset").toObject().toVariantMap();
  case TradableRole:
    return item.value("tradable").toBool();
  case MarketNameRole:
    return marketName(item);
  case SellableRole:
    return item.value("tradable").toBool() &&
           item.value("quantity").toInt() > 0;
  case PlatinumRole: {
    const QJsonValue price =
        marketQuotes_.value(marketName(item)).value("lowest_sell");
    return price.isDouble() ? QVariant(price.toInt()) : QVariant();
  }
  case BuyPlatinumRole: {
    const QJsonValue price =
        marketQuotes_.value(marketName(item)).value("highest_buy");
    return price.isDouble() ? QVariant(price.toInt()) : QVariant();
  }
  case PriceStateRole: {
    if (!item.value("tradable").toBool()) {
      return QStringLiteral("none");
    }
    const QString name = marketName(item);
    if (marketQuotes_.contains(name)) {
      return QStringLiteral("ready");
    }
    return unavailableMarketItems_.contains(name) ||
                   marketQuotes_.contains(name)
               ? QStringLiteral("unavailable")
               : QStringLiteral("loading");
  }
  case IsPrimeRole:
    return item.value("is_prime").toBool();
  case MasteryRequirementRole:
    return item.value("mastery_requirement").toInt();
  case ReadyToBuildRole:
    return item.value("ready_to_build").toBool();
  case FavoriteRole: {
    const auto favorite =
        favoriteOverrides_.constFind(item.value("id").toString());
    return favorite == favoriteOverrides_.cend()
               ? item.value("favorite").toBool()
               : favorite.value();
  }
  case VaultedRole:
    return item.value("vaulted").isBool()
               ? QVariant(item.value("vaulted").toBool())
               : QVariant();
  case SubsumedRole:
    return item.value("subsumed").toBool();
  default:
    return {};
  }
}

bool PlayerItemModel::setData(const QModelIndex &index, const QVariant &value,
                              int role) {
  if (role != FavoriteRole || !index.isValid() || index.row() < 0 ||
      index.row() >= items_.size()) {
    return false;
  }
  const QString id = items_.at(index.row()).value("id").toString();
  if (id.isEmpty()) {
    return false;
  }
  const bool favorite = value.toBool();
  if (data(index, FavoriteRole).toBool() == favorite) {
    return false;
  }
  favoriteOverrides_.insert(id, favorite);
  emit dataChanged(index, index, {FavoriteRole});
  return true;
}

QHash<int, QByteArray> PlayerItemModel::roleNames() const {
  return {{NameRole, "name"},
          {IdRole, "id"},
          {GroupRole, "group"},
          {CategoryRole, "category"},
          {TypeRole, "type"},
          {QuantityRole, "quantity"},
          {DucatsRole, "ducats"},
          {MasteredRole, "mastered"},
          {AssetPathRole, "assetPath"},
          {OwnedRole, "owned"},
          {PendingRole, "pending"},
          {RankRole, "rank"},
          {MaxRankRole, "maxRank"},
          {PotentialXpRole, "potentialXp"},
          {MissingPartsRole, "missingParts"},
          {FromRelicsRole, "fromRelics"},
          {RelicProbabilityRole, "relicProbability"},
          {BuyableRole, "buyable"},
          {AcquisitionPlatinumRole, "acquisitionPlatinum"},
          {AcquisitionPriceStateRole, "acquisitionPriceState"},
          {HasRecipeRole, "hasRecipe"},
          {ComponentsRole, "components"},
          {AssetSpecRole, "assetSpec"},
          {TradableRole, "tradable"},
          {MarketNameRole, "marketName"},
          {SellableRole, "sellable"},
          {PlatinumRole, "platinum"},
          {BuyPlatinumRole, "buyPlatinum"},
          {PriceStateRole, "priceState"},
          {IsPrimeRole, "isPrime"},
          {MasteryRequirementRole, "masteryRequirement"},
          {ReadyToBuildRole, "readyToBuild"},
          {FavoriteRole, "favorite"},
          {VaultedRole, "vaulted"},
          {SubsumedRole, "subsumed"},
          {AssetRefRole, "assetRef"}};
}

bool PlayerItemModel::replace(const QJsonObject &data, QString *error) {
  if (!data.value("items").isArray()) {
    if (error) {
      *error = "daemon returned malformed player view data";
    }
    return false;
  }
  QList<QJsonObject> next;
  for (const QJsonValue &value : data.value("items").toArray()) {
    const QJsonObject item = value.toObject();
    if (!item.value("id").isString() || !item.value("name").isString()) {
      continue;
    }
    next.append(item);
  }
  beginResetModel();
  items_ = std::move(next);
  componentCache_.clear();
  rebuildIndexes();
  endResetModel();
  return true;
}

void PlayerItemModel::clear() {
  if (items_.isEmpty()) {
    return;
  }
  beginResetModel();
  items_.clear();
  assetRows_.clear();
  nameRows_.clear();
  componentCache_.clear();
  endResetModel();
}

void PlayerItemModel::setAssets(const wfgui::AssetMap &assets) {
  if (assets_ == assets) {
    return;
  }

  QSet<int> affectedRows;
  for (auto asset = assets_.cbegin(); asset != assets_.cend(); ++asset) {
    if (assets.value(asset.key()) != asset.value()) {
      for (int row : assetRows_.values(asset.key())) {
        affectedRows.insert(row);
      }
    }
  }
  for (auto asset = assets.cbegin(); asset != assets.cend(); ++asset) {
    if (assets_.value(asset.key()) != asset.value()) {
      for (int row : assetRows_.values(asset.key())) {
        affectedRows.insert(row);
      }
    }
  }

  assets_ = assets;
  for (int row : affectedRows) {
    componentCache_.remove(row);
  }
  notifyRows(affectedRows, {AssetPathRole, AssetRefRole, ComponentsRole});
}

void PlayerItemModel::applyAssets(const wfgui::AssetMap &assets) {
  QSet<int> affectedRows;
  for (auto asset = assets.cbegin(); asset != assets.cend(); ++asset) {
    if (!asset.value().isValid() ||
        assets_.value(asset.key()) == asset.value()) {
      continue;
    }
    assets_.insert(asset.key(), asset.value());
    for (int row : assetRows_.values(asset.key())) {
      affectedRows.insert(row);
    }
  }
  for (int row : affectedRows) {
    componentCache_.remove(row);
  }
  notifyRows(affectedRows, {AssetPathRole, AssetRefRole, ComponentsRole});
}

void PlayerItemModel::applyMarketQuotes(const QJsonArray &quotes,
                                        const QJsonArray &missing) {
  QSet<QString> changed;
  for (const QJsonValue &value : quotes) {
    const QJsonObject row = value.toObject();
    const QString name = row.value("item").toString();
    if (name.isEmpty()) {
      continue;
    }
    const QJsonValue quote = row.value("quote");
    if (quote.isObject()) {
      const QJsonObject next = quote.toObject();
      if (marketQuotes_.value(name) == next &&
          !unavailableMarketItems_.contains(name)) {
        continue;
      }
      marketQuotes_.insert(name, next);
      unavailableMarketItems_.remove(name);
    } else {
      if (!marketQuotes_.contains(name) &&
          unavailableMarketItems_.contains(name)) {
        continue;
      }
      marketQuotes_.remove(name);
      unavailableMarketItems_.insert(name);
    }
    changed.insert(name);
  }
  for (const QJsonValue &value : missing) {
    const QString name = value.toString();
    if (!name.isEmpty()) {
      if (!marketQuotes_.contains(name) &&
          unavailableMarketItems_.contains(name)) {
        continue;
      }
      marketQuotes_.remove(name);
      unavailableMarketItems_.insert(name);
      changed.insert(name);
    }
  }
  notifyMarketRows(changed);
}

void PlayerItemModel::markMarketUnavailable(const QStringList &items) {
  QSet<QString> changed;
  for (const QString &name : items) {
    if (!name.isEmpty()) {
      unavailableMarketItems_.insert(name);
      changed.insert(name);
    }
  }
  notifyMarketRows(changed);
}

void PlayerItemModel::rebuildIndexes() {
  assetRows_.clear();
  nameRows_.clear();
  for (int row = 0; row < items_.size(); ++row) {
    const QJsonObject &item = items_.at(row);
    const QString name = item.value("name").toString();
    nameRows_.insert(name, row);
    const QString quoteName = marketName(item);
    if (quoteName != name) {
      nameRows_.insert(quoteName, row);
    }
    const QString itemAsset = assetId(item);
    if (!itemAsset.isEmpty()) {
      assetRows_.insert(itemAsset, row);
    }
    for (const QJsonValue &value : item.value("components").toArray()) {
      const QJsonObject component = value.toObject();
      const QString componentAsset = assetId(component);
      if (!componentAsset.isEmpty()) {
        assetRows_.insert(componentAsset, row);
      }
      const QString marketName = component.value("market_name").toString();
      if (!marketName.isEmpty()) {
        nameRows_.insert(marketName, row);
      }
    }
  }
}

void PlayerItemModel::notifyMarketRows(const QSet<QString> &names) {
  QSet<int> rows;
  for (const QString &name : names) {
    for (int row : nameRows_.values(name)) {
      rows.insert(row);
    }
  }
  notifyRows(rows, {PlatinumRole, BuyPlatinumRole, PriceStateRole,
                    AcquisitionPlatinumRole, AcquisitionPriceStateRole});
}

void PlayerItemModel::notifyRows(const QSet<int> &rows,
                                 const QList<int> &roles) {
  QList<int> sorted = rows.values();
  std::sort(sorted.begin(), sorted.end());
  for (qsizetype first = 0; first < sorted.size();) {
    qsizetype last = first;
    while (last + 1 < sorted.size() &&
           sorted.at(last + 1) == sorted.at(last) + 1) {
      ++last;
    }
    emit dataChanged(index(sorted.at(first)), index(sorted.at(last)), roles);
    first = last + 1;
  }
}

PlayerItemFilterModel::PlayerItemFilterModel(QObject *parent)
    : QSortFilterProxyModel(parent) {
  setDynamicSortFilter(false);
  sort(0);
}

QVariant PlayerItemFilterModel::data(const QModelIndex &index, int role) const {
  QVariant value = QSortFilterProxyModel::data(index, role);
  if (role != PlayerItemModel::ComponentsRole || text_.isEmpty()) {
    return value;
  }
  QVariantList components;
  for (const QVariant &entry : value.toList()) {
    QVariantMap component = entry.toMap();
    const bool match = component.value("name").toString().contains(
                           text_, Qt::CaseInsensitive) ||
                       component.value("market_name")
                           .toString()
                           .contains(text_, Qt::CaseInsensitive);
    component.insert("search_match", match);
    components.push_back(component);
  }
  return components;
}

void PlayerItemFilterModel::setText(const QString &text) {
  if (text_ == text) {
    return;
  }
  beginFilterChange();
  text_ = text;
  endFilterChange(QSortFilterProxyModel::Direction::Rows);
  if (rowCount() > 0) {
    emit dataChanged(index(0, 0), index(rowCount() - 1, 0),
                     {PlayerItemModel::ComponentsRole});
  }
}

void PlayerItemFilterModel::setGroup(const QString &group) {
  if (group_ == group) {
    return;
  }
  beginFilterChange();
  group_ = group;
  endFilterChange(QSortFilterProxyModel::Direction::Rows);
}

void PlayerItemFilterModel::setMode(const QString &mode) {
  if (mode_ == mode) {
    return;
  }
  beginFilterChange();
  mode_ = mode;
  endFilterChange(QSortFilterProxyModel::Direction::Rows);
  sort(0);
}

void PlayerItemFilterModel::setPricesLoading(bool loading) {
  if (pricesLoading_ == loading) {
    return;
  }
  beginFilterChange();
  pricesLoading_ = loading;
  endFilterChange(QSortFilterProxyModel::Direction::Rows);
  sort(0);
}

void PlayerItemFilterModel::setFlag(const QString &name, int state) {
  if (state < 0) {
    if (!flags_.contains(name)) {
      return;
    }
  } else if (flags_.value(name, -1) == state) {
    return;
  }
  beginFilterChange();
  if (state < 0) {
    flags_.remove(name);
  } else {
    flags_.insert(name, state);
  }
  endFilterChange(QSortFilterProxyModel::Direction::Rows);
}

void PlayerItemFilterModel::setSortMode(const QString &mode) {
  if (sortMode_ == mode) {
    return;
  }
  sortMode_ = mode;
  invalidate();
  sort(0);
}

void PlayerItemFilterModel::setSortAscending(bool ascending) {
  if (sortAscending_ == ascending) {
    return;
  }
  sortAscending_ = ascending;
  invalidate();
  sort(0);
}

bool PlayerItemFilterModel::filterAcceptsRow(
    int sourceRow, const QModelIndex &sourceParent) const {
  const QModelIndex item = sourceModel()->index(sourceRow, 0, sourceParent);
  if (group_ != "all" &&
      sourceModel()->data(item, PlayerItemModel::GroupRole).toString() !=
          group_) {
    return false;
  }
  if (!text_.isEmpty()) {
    bool matches = sourceModel()
                       ->data(item, PlayerItemModel::NameRole)
                       .toString()
                       .contains(text_, Qt::CaseInsensitive);
    if (!matches) {
      const QVariantList components =
          sourceModel()->data(item, PlayerItemModel::ComponentsRole).toList();
      matches = std::ranges::any_of(components, [this](const QVariant &entry) {
        const QVariantMap component = entry.toMap();
        return component.value("name").toString().contains(
                   text_, Qt::CaseInsensitive) ||
               component.value("market_name")
                   .toString()
                   .contains(text_, Qt::CaseInsensitive);
      });
    }
    if (!matches) {
      return false;
    }
  }
  const bool mastered =
      sourceModel()->data(item, PlayerItemModel::MasteredRole).toBool();
  for (auto flag = flags_.cbegin(); flag != flags_.cend(); ++flag) {
    const int role = flagRole(flag.key());
    bool actual = false;
    if (flag.key() == "duplicate") {
      actual =
          sourceModel()->data(item, PlayerItemModel::QuantityRole).toInt() > 1;
    } else if (flag.key() == "complete") {
      const QVariantList components =
          sourceModel()->data(item, PlayerItemModel::ComponentsRole).toList();
      actual = !components.isEmpty() &&
               std::all_of(components.cbegin(), components.cend(),
                           [](const QVariant &value) {
                             const QVariantMap component = value.toMap();
                             return component.value("owned").toInt() >=
                                    component.value("required").toInt();
                           });
    } else if (role >= 0) {
      const QVariant value = sourceModel()->data(item, role);
      if (role == PlayerItemModel::VaultedRole && !value.isValid()) {
        return false;
      }
      actual = value.toBool();
    } else {
      continue;
    }
    if (actual != (flag.value() == 1)) {
      return false;
    }
  }
  if (mode_ == "easy") {
    const bool owned =
        sourceModel()->data(item, PlayerItemModel::OwnedRole).toBool();
    const bool ready =
        sourceModel()->data(item, PlayerItemModel::HasRecipeRole).toBool() &&
        sourceModel()->data(item, PlayerItemModel::MissingPartsRole).toInt() ==
            0;
    return !mastered &&
           sourceModel()->data(item, PlayerItemModel::RankRole).toInt() < 30 &&
           (owned || ready);
  }
  if (mode_ == "relics") {
    return !mastered &&
           sourceModel()->data(item, PlayerItemModel::FromRelicsRole).toBool();
  }
  if (mode_ == "platinum") {
    if (mastered ||
        !sourceModel()->data(item, PlayerItemModel::BuyableRole).toBool()) {
      return false;
    }
    if (pricesLoading_) {
      return true;
    }
    const QString state =
        sourceModel()
            ->data(item, PlayerItemModel::AcquisitionPriceStateRole)
            .toString();
    return state == "loading" ||
           (state == "ready" &&
            sourceModel()
                    ->data(item, PlayerItemModel::AcquisitionPlatinumRole)
                    .toInt() > 0);
  }
  return true;
}

bool PlayerItemFilterModel::lessThan(const QModelIndex &left,
                                     const QModelIndex &right) const {
  const auto value = [this](const QModelIndex &index, int role) {
    return sourceModel()->data(index, role);
  };
  const QString leftName = value(left, PlayerItemModel::NameRole).toString();
  const QString rightName = value(right, PlayerItemModel::NameRole).toString();
  if (mode_ == "easy") {
    return std::tuple(!value(left, PlayerItemModel::OwnedRole).toBool(),
                      value(left, PlayerItemModel::MissingPartsRole).toInt(),
                      -value(left, PlayerItemModel::PotentialXpRole).toInt(),
                      leftName.toCaseFolded()) <
           std::tuple(!value(right, PlayerItemModel::OwnedRole).toBool(),
                      value(right, PlayerItemModel::MissingPartsRole).toInt(),
                      -value(right, PlayerItemModel::PotentialXpRole).toInt(),
                      rightName.toCaseFolded());
  }
  if (mode_ == "relics") {
    return std::tuple(
               -value(left, PlayerItemModel::RelicProbabilityRole).toDouble(),
               value(left, PlayerItemModel::MissingPartsRole).toInt(),
               leftName.toCaseFolded()) <
           std::tuple(
               -value(right, PlayerItemModel::RelicProbabilityRole).toDouble(),
               value(right, PlayerItemModel::MissingPartsRole).toInt(),
               rightName.toCaseFolded());
  }
  if (mode_ == "platinum") {
    const QVariant leftPrice =
        value(left, PlayerItemModel::AcquisitionPlatinumRole);
    const QVariant rightPrice =
        value(right, PlayerItemModel::AcquisitionPlatinumRole);
    const double leftEfficiency =
        leftPrice.isValid() && leftPrice.toInt() > 0
            ? -value(left, PlayerItemModel::PotentialXpRole).toDouble() /
                  leftPrice.toInt()
            : 0;
    const double rightEfficiency =
        rightPrice.isValid() && rightPrice.toInt() > 0
            ? -value(right, PlayerItemModel::PotentialXpRole).toDouble() /
                  rightPrice.toInt()
            : 0;
    return std::tuple(!leftPrice.isValid(), leftEfficiency,
                      leftName.toCaseFolded()) <
           std::tuple(!rightPrice.isValid(), rightEfficiency,
                      rightName.toCaseFolded());
  }
  if (pricesLoading_ && (sortMode_ == "platinum" || sortMode_ == "ducanator")) {
    const int nameOrder = leftName.compare(rightName, Qt::CaseInsensitive);
    return sortAscending_ ? nameOrder < 0 : nameOrder > 0;
  }
  const auto sortValue = [this, &value](const QModelIndex &item) -> QVariant {
    if (sortMode_ == "platinum") {
      return value(item, PlayerItemModel::PlatinumRole);
    }
    if (sortMode_ == "ducats") {
      return value(item, PlayerItemModel::DucatsRole);
    }
    if (sortMode_ == "amount") {
      return value(item, PlayerItemModel::QuantityRole);
    }
    if (sortMode_ == "ducanator") {
      const QVariant platinum = value(item, PlayerItemModel::PlatinumRole);
      if (!platinum.isValid() || platinum.toDouble() <= 0) {
        return {};
      }
      return value(item, PlayerItemModel::DucatsRole).toDouble() /
             platinum.toDouble();
    }
    if (sortMode_ == "complete") {
      const QVariantList components =
          value(item, PlayerItemModel::ComponentsRole).toList();
      if (components.isEmpty()) {
        return {};
      }
      int complete = 0;
      for (const QVariant &entry : components) {
        const QVariantMap component = entry.toMap();
        complete += component.value("owned").toInt() >=
                    component.value("required").toInt();
      }
      return static_cast<double>(complete) / components.size();
    }
    return {};
  };
  if (sortMode_ != "name") {
    const QVariant leftValue = sortValue(left);
    const QVariant rightValue = sortValue(right);
    if (leftValue.isValid() != rightValue.isValid()) {
      return leftValue.isValid();
    }
    if (leftValue.isValid() && leftValue.toDouble() != rightValue.toDouble()) {
      return sortAscending_ ? leftValue.toDouble() < rightValue.toDouble()
                            : leftValue.toDouble() > rightValue.toDouble();
    }
  }
  const int nameOrder = leftName.compare(rightName, Qt::CaseInsensitive);
  return sortAscending_ ? nameOrder < 0 : nameOrder > 0;
}
