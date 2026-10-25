#include "relic_model.h"

#include <QList>
#include <QSet>
#include <QVariantList>
#include <QVariantMap>

#include <algorithm>
#include <ranges>
#include <utility>

import wfgui.relic_data;

struct RelicModel::Storage {
  wfgui::RelicData data;
};

namespace {
QString identity(const wfgui::Relic &relic) {
  return relic.id.isEmpty() ? relic.name : relic.id;
}

bool sameRelics(const wfgui::RelicData &current, const wfgui::RelicData &next) {
  if (current.relics.size() != next.relics.size()) {
    return false;
  }
  QSet<QString> identities;
  for (const wfgui::Relic &relic : next.relics) {
    const QString key = identity(relic);
    if (key.isEmpty() || identities.contains(key)) {
      return false;
    }
    identities.insert(key);
  }
  return std::ranges::all_of(current.relics, [&identities](const auto &relic) {
    return identities.contains(identity(relic));
  });
}

void preserveAssetPaths(const wfgui::Relic &current, wfgui::Relic &next) {
  if (current.assetId == next.assetId) {
    next.assetPath = current.assetPath;
  }
  QHash<QString, QString> rewardPaths;
  for (const wfgui::Relic::Reward &reward : current.rewards) {
    if (!reward.assetPath.isEmpty()) {
      rewardPaths.insert(reward.assetId, reward.assetPath);
    }
  }
  for (wfgui::Relic::Reward &reward : next.rewards) {
    reward.assetPath = rewardPaths.value(reward.assetId);
  }
}
} // namespace

RelicModel::RelicModel(QObject *parent)
    : QAbstractListModel(parent), storage_(std::make_unique<Storage>()) {}

RelicModel::~RelicModel() = default;

int RelicModel::rowCount(const QModelIndex &parent) const {
  return parent.isValid() ? 0 : static_cast<int>(storage_->data.relics.size());
}

QVariant RelicModel::data(const QModelIndex &index, int role) const {
  if (!index.isValid() || index.row() < 0 ||
      index.row() >= static_cast<int>(storage_->data.relics.size())) {
    return {};
  }

  const wfgui::Relic &relic =
      storage_->data.relics[static_cast<std::size_t>(index.row())];
  switch (role) {
  case NameRole:
    return relic.name;
  case AmountOwnedRole:
    return relic.amountOwned;
  case VaultedRole:
    return relic.vaulted;
  case FavoriteRole:
    return relic.favorite;
  case HasPriceRole:
    return relic.hasPrice;
  case ExpectedPlatinumRole:
    return relic.expectedPlatinum;
  case ExpectedDucatsRole:
    return relic.expectedDucats;
  case PricesLoadingRole:
    return pricesLoading_;
  case RefinementRole:
    return relic.refinement;
  case RelicImageRole:
    return assets_.value(relic.assetId).path;
  case RelicAssetRole:
    return QVariant::fromValue(assets_.value(relic.assetId));
  case PriceCompleteRole:
    return relic.priceComplete;
  case RefinementsRole: {
    QVariantList refinements;
    refinements.reserve(static_cast<qsizetype>(relic.refinements.size()));
    for (const wfgui::Relic::Refinement &refinement : relic.refinements) {
      refinements.push_back(QVariantMap{
          {"name", refinement.name},
          {"amountOwned", refinement.amountOwned},
          {"expectedPlatinum", refinement.expectedPlatinum},
          {"expectedDucats", refinement.expectedDucats},
          {"hasPrice", refinement.hasPrice},
          {"priceComplete", refinement.priceComplete},
      });
    }
    return refinements;
  }
  case RewardsRole: {
    QVariantList rewards;
    rewards.reserve(static_cast<qsizetype>(relic.rewards.size()));
    for (const wfgui::Relic::Reward &reward : relic.rewards) {
      rewards.push_back(QVariantMap{
          {"name", reward.name},
          {"rarity", reward.rarity},
          {"image", assets_.value(reward.assetId).path},
          {"assetRef", QVariant::fromValue(assets_.value(reward.assetId))},
          {"platinum", reward.platinum},
          {"ducats", reward.ducats},
          {"owned", reward.owned},
          {"chance", reward.chance},
          {"hasPrice", reward.hasPrice},
      });
    }
    return rewards;
  }
  case EraRole:
    return relic.era;
  default:
    return {};
  }
}

QHash<int, QByteArray> RelicModel::roleNames() const {
  return {
      {NameRole, "name"},
      {AmountOwnedRole, "amountOwned"},
      {VaultedRole, "vaulted"},
      {FavoriteRole, "favorite"},
      {HasPriceRole, "hasPrice"},
      {ExpectedPlatinumRole, "expectedPlatinum"},
      {ExpectedDucatsRole, "expectedDucats"},
      {PricesLoadingRole, "pricesLoading"},
      {RefinementRole, "refinement"},
      {RelicImageRole, "relicImage"},
      {PriceCompleteRole, "priceComplete"},
      {RefinementsRole, "refinements"},
      {RewardsRole, "rewards"},
      {EraRole, "era"},
      {RelicAssetRole, "relicAsset"},
  };
}

void RelicModel::clear() {
  if (storage_->data.relics.empty() && storage_->data.traceCount == 0) {
    return;
  }
  beginResetModel();
  storage_->data = {};
  assetRows_.clear();
  endResetModel();
}

void RelicModel::setPricesLoading(bool loading) {
  if (pricesLoading_ == loading) {
    return;
  }
  pricesLoading_ = loading;
  if (rowCount() > 0) {
    emit dataChanged(index(0), index(rowCount() - 1), {PricesLoadingRole});
  }
}

void RelicModel::setAssets(const wfgui::AssetMap &assets) {
  if (assets_ == assets) {
    return;
  }
  QSet<int> changedRows;
  QSet<QString> ids;
  for (auto asset = assets_.cbegin(); asset != assets_.cend(); ++asset) {
    if (assets.value(asset.key()) != asset.value()) {
      ids.insert(asset.key());
    }
  }
  for (auto asset = assets.cbegin(); asset != assets.cend(); ++asset) {
    if (assets_.value(asset.key()) != asset.value()) {
      ids.insert(asset.key());
    }
  }
  for (const QString &id : std::as_const(ids)) {
    for (int row : assetRows_.values(id)) {
      changedRows.insert(row);
    }
  }
  assets_ = assets;
  notifyAssetRows(changedRows);
}

void RelicModel::applyAssets(const wfgui::AssetMap &assets) {
  QSet<int> changedRows;
  for (auto asset = assets.cbegin(); asset != assets.cend(); ++asset) {
    if (!asset.value().isValid() ||
        assets_.value(asset.key()) == asset.value()) {
      continue;
    }
    assets_.insert(asset.key(), asset.value());
    for (int row : assetRows_.values(asset.key())) {
      changedRows.insert(row);
    }
  }
  notifyAssetRows(changedRows);
}

bool RelicModel::replace(const QJsonObject &data, QString *error) {
  wfgui::RelicData next;
  if (!wfgui::parseRelicData(data, next, error)) {
    return false;
  }

  if (sameRelics(storage_->data, next)) {
    QHash<QString, std::size_t> rows;
    for (std::size_t row = 0; row < next.relics.size(); ++row) {
      rows.insert(identity(next.relics[row]), row);
    }
    for (wfgui::Relic &current : storage_->data.relics) {
      wfgui::Relic updated =
          std::move(next.relics[rows.value(identity(current))]);
      preserveAssetPaths(current, updated);
      current = std::move(updated);
    }
    storage_->data.traceCount = next.traceCount;
    rebuildAssetIndex();
    if (rowCount() > 0) {
      emit dataChanged(index(0), index(rowCount() - 1));
    }
    return true;
  }

  beginResetModel();
  storage_->data = std::move(next);
  rebuildAssetIndex();
  endResetModel();
  return true;
}

void RelicModel::rebuildAssetIndex() {
  assetRows_.clear();
  for (int row = 0; row < rowCount(); ++row) {
    const wfgui::Relic &relic =
        storage_->data.relics[static_cast<std::size_t>(row)];
    if (!relic.assetId.isEmpty()) {
      assetRows_.insert(relic.assetId, row);
    }
    for (const wfgui::Relic::Reward &reward : relic.rewards) {
      if (!reward.assetId.isEmpty()) {
        assetRows_.insert(reward.assetId, row);
      }
    }
  }
}

void RelicModel::notifyAssetRows(const QSet<int> &rows) {
  QList<int> sorted = rows.values();
  std::sort(sorted.begin(), sorted.end());
  for (qsizetype first = 0; first < sorted.size();) {
    qsizetype last = first;
    while (last + 1 < sorted.size() &&
           sorted.at(last + 1) == sorted.at(last) + 1) {
      ++last;
    }
    emit dataChanged(index(sorted.at(first)), index(sorted.at(last)),
                     {RelicImageRole, RelicAssetRole, RewardsRole});
    first = last + 1;
  }
}

int RelicModel::traceCount() const { return storage_->data.traceCount; }

RelicFilterModel::RelicFilterModel(QObject *parent)
    : QSortFilterProxyModel(parent) {
  setDynamicSortFilter(true);
}

QString RelicFilterModel::filterText() const { return filterText_; }

bool RelicFilterModel::onlyOwned() const { return onlyOwned_; }

QString RelicFilterModel::era() const { return era_; }

QVariant RelicFilterModel::data(const QModelIndex &index, int role) const {
  QVariant value = QSortFilterProxyModel::data(index, role);
  if (role == RelicModel::RewardsRole && !filterText_.isEmpty()) {
    QVariantList rewards;
    for (const QVariant &entry : value.toList()) {
      QVariantMap reward = entry.toMap();
      reward.insert("search_match", reward.value("name").toString().contains(
                                        filterText_, Qt::CaseInsensitive));
      rewards.push_back(reward);
    }
    return rewards;
  }
  if (!onlyOwned_ || role != RelicModel::RefinementsRole) {
    return value;
  }

  QVariantList owned;
  for (const QVariant &row : value.toList()) {
    if (row.toMap().value("amountOwned").toInt() > 0) {
      owned.push_back(row);
    }
  }
  return owned;
}

void RelicFilterModel::setFilterText(const QString &text) {
  if (filterText_ == text) {
    return;
  }
  beginFilterChange();
  filterText_ = text;
  endFilterChange(QSortFilterProxyModel::Direction::Rows);
  if (rowCount() > 0) {
    emit dataChanged(index(0, 0), index(rowCount() - 1, 0),
                     {RelicModel::RewardsRole});
  }
  emit filterTextChanged();
}

void RelicFilterModel::setOnlyOwned(bool onlyOwned) {
  if (onlyOwned_ == onlyOwned) {
    return;
  }
  beginFilterChange();
  onlyOwned_ = onlyOwned;
  endFilterChange(QSortFilterProxyModel::Direction::Rows);
  if (rowCount() > 0) {
    emit dataChanged(index(0, 0), index(rowCount() - 1, 0),
                     {RelicModel::RefinementsRole});
  }
  emit onlyOwnedChanged();
}

void RelicFilterModel::setEra(const QString &era) {
  if (era_ == era) {
    return;
  }
  beginFilterChange();
  era_ = era;
  endFilterChange(QSortFilterProxyModel::Direction::Rows);
  emit eraChanged();
}

bool RelicFilterModel::filterAcceptsRow(int sourceRow,
                                        const QModelIndex &sourceParent) const {
  const QModelIndex index = sourceModel()->index(sourceRow, 0, sourceParent);
  if (onlyOwned_ &&
      sourceModel()->data(index, RelicModel::AmountOwnedRole).toInt() <= 0) {
    return false;
  }
  if (era_ != "all" &&
      sourceModel()->data(index, RelicModel::EraRole).toString() != era_) {
    return false;
  }
  if (filterText_.isEmpty() ||
      sourceModel()
          ->data(index, RelicModel::NameRole)
          .toString()
          .contains(filterText_, Qt::CaseInsensitive)) {
    return true;
  }
  const QVariantList rewards =
      sourceModel()->data(index, RelicModel::RewardsRole).toList();
  return std::ranges::any_of(rewards, [this](const QVariant &value) {
    return value.toMap().value("name").toString().contains(filterText_,
                                                           Qt::CaseInsensitive);
  });
}
