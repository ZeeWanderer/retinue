#include "build_equipment_model.h"

#include <QHash>
#include <QSet>

#include <algorithm>

BuildEquipmentModel::BuildEquipmentModel(QObject *parent)
    : QAbstractListModel(parent) {}

int BuildEquipmentModel::rowCount(const QModelIndex &parent) const {
  return parent.isValid() ? 0 : groups_.size();
}

QVariant BuildEquipmentModel::data(const QModelIndex &index, int role) const {
  if (!index.isValid() || index.row() < 0 || index.row() >= groups_.size()) {
    return {};
  }
  const Group &group = groups_.at(index.row());
  const QJsonObject &definition = group.definition;
  const QJsonObject asset = definition.value("asset").toObject();
  const QString assetId = asset.value("id").toString();
  switch (role) {
  case Qt::DisplayRole:
  case NameRole:
    return definition.value("name").toString();
  case IdRole:
    return definition.value("id").toString();
  case ClassRole:
    return definition.value("class").toString();
  case CategoryRole:
    return definition.value("category").toString();
  case AssetSpecRole:
    return asset.toVariantMap();
  case AssetPathRole:
    return assets_.value(assetId).path;
  case InstancesRole:
    return group.instances.toVariantList();
  case InstanceCountRole:
    return group.instances.size();
  default:
    return {};
  }
}

QHash<int, QByteArray> BuildEquipmentModel::roleNames() const {
  return {{IdRole, "id"},
          {NameRole, "name"},
          {ClassRole, "class"},
          {CategoryRole, "category"},
          {AssetSpecRole, "assetSpec"},
          {AssetPathRole, "assetPath"},
          {InstancesRole, "instances"},
          {InstanceCountRole, "instanceCount"}};
}

bool BuildEquipmentModel::replace(const QJsonObject &data, QString *error) {
  if (!data.value("definitions").isArray() ||
      !data.value("instances").isArray()) {
    if (error) {
      *error = "daemon returned malformed build equipment data";
    }
    return false;
  }

  QHash<QString, Group> groups;
  for (const QJsonValue &value : data.value("definitions").toArray()) {
    const QJsonObject definition = value.toObject();
    const QString id = definition.value("id").toString();
    if (id.isEmpty() || definition.value("name").toString().isEmpty()) {
      if (error) {
        *error = "build equipment definition is missing identity";
      }
      return false;
    }
    groups.insert(id, Group{.definition = definition, .instances = {}});
  }

  for (const QJsonValue &value : data.value("instances").toArray()) {
    const QJsonObject instance = value.toObject();
    const QString id = instance.value("instance_id").toString();
    const QString definitionId = instance.value("definition_id").toString();
    if (id.isEmpty() || !groups.contains(definitionId)) {
      if (error) {
        *error = "build equipment instance has no matching definition";
      }
      return false;
    }
    groups[definitionId].instances.append(instance);
  }

  QList<Group> next;
  next.reserve(groups.size());
  for (Group &group : groups) {
    if (!group.instances.isEmpty()) {
      next.append(std::move(group));
    }
  }
  std::ranges::sort(next, [](const Group &left, const Group &right) {
    return QString::localeAwareCompare(
               left.definition.value("name").toString(),
               right.definition.value("name").toString()) < 0;
  });

  beginResetModel();
  groups_ = std::move(next);
  revision_ = data.value("player_revision").toInteger(-1);
  updatedAt_ = data.value("player_updated_at").toInteger(0);
  endResetModel();
  return true;
}

void BuildEquipmentModel::clear() {
  beginResetModel();
  groups_.clear();
  revision_ = -1;
  updatedAt_ = 0;
  endResetModel();
}

void BuildEquipmentModel::setAssets(const wfgui::AssetMap &assets) {
  assets_ = assets;
  if (!groups_.isEmpty()) {
    emit dataChanged(index(0), index(groups_.size() - 1), {AssetPathRole});
  }
}

void BuildEquipmentModel::applyAssets(const wfgui::AssetMap &assets) {
  QSet<int> changed;
  for (auto asset = assets.cbegin(); asset != assets.cend(); ++asset) {
    if (assets_.value(asset.key()) == asset.value()) {
      continue;
    }
    assets_.insert(asset.key(), asset.value());
    for (int row = 0; row < groups_.size(); ++row) {
      const QString id = groups_.at(row)
                             .definition.value("asset")
                             .toObject()
                             .value("id")
                             .toString();
      if (id == asset.key()) {
        changed.insert(row);
      }
    }
  }
  for (int row : changed) {
    emit dataChanged(index(row), index(row), {AssetPathRole});
  }
}

qint64 BuildEquipmentModel::revision() const { return revision_; }

qint64 BuildEquipmentModel::updatedAt() const { return updatedAt_; }

BuildEquipmentFilterModel::BuildEquipmentFilterModel(QObject *parent)
    : QSortFilterProxyModel(parent) {
  setDynamicSortFilter(true);
  setSortCaseSensitivity(Qt::CaseInsensitive);
  sort(0);
}

void BuildEquipmentFilterModel::setText(const QString &text) {
  const QString normalized = text.trimmed();
  if (text_ == normalized) {
    return;
  }
  beginFilterChange();
  text_ = normalized;
  endFilterChange(Direction::Rows);
}

void BuildEquipmentFilterModel::setCategory(const QString &category) {
  const QString normalized = category.trimmed().toLower();
  if (category_ == normalized) {
    return;
  }
  beginFilterChange();
  category_ = normalized.isEmpty() ? "all" : normalized;
  endFilterChange(Direction::Rows);
}

bool BuildEquipmentFilterModel::filterAcceptsRow(
    int sourceRow, const QModelIndex &sourceParent) const {
  const QModelIndex index = sourceModel()->index(sourceRow, 0, sourceParent);
  const QString itemClass = index.data(BuildEquipmentModel::ClassRole).toString();
  if (category_ != "all" && itemClass != category_) {
    return false;
  }
  if (text_.isEmpty()) {
    return true;
  }
  if (index.data(BuildEquipmentModel::NameRole)
          .toString()
          .contains(text_, Qt::CaseInsensitive) ||
      itemClass.contains(text_, Qt::CaseInsensitive) ||
      index.data(BuildEquipmentModel::IdRole)
          .toString()
          .contains(text_, Qt::CaseInsensitive)) {
    return true;
  }
  const QVariantList instances =
      index.data(BuildEquipmentModel::InstancesRole).toList();
  return std::ranges::any_of(instances, [this](const QVariant &value) {
    return value.toMap()
        .value("custom_name")
        .toString()
        .contains(text_, Qt::CaseInsensitive);
  });
}

bool BuildEquipmentFilterModel::lessThan(const QModelIndex &left,
                                         const QModelIndex &right) const {
  return QString::localeAwareCompare(
             left.data(BuildEquipmentModel::NameRole).toString(),
             right.data(BuildEquipmentModel::NameRole).toString()) < 0;
}
