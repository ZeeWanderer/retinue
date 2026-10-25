#include "build_group_model.h"

#include <algorithm>

namespace {
bool groupBefore(const QJsonObject &left, const QJsonObject &right) {
  const qint64 leftUpdated = left.value("updated_at").toInteger();
  const qint64 rightUpdated = right.value("updated_at").toInteger();
  if (leftUpdated != rightUpdated) {
    return leftUpdated > rightUpdated;
  }
  const int byName = QString::localeAwareCompare(
      left.value("name").toString(), right.value("name").toString());
  if (byName != 0) {
    return byName < 0;
  }
  return left.value("id").toString() < right.value("id").toString();
}

bool hasMemberSnapshots(const QJsonObject &group) {
  for (const QJsonValue &value : group.value("members").toArray()) {
    if (value.toObject().value("snapshot").isObject()) {
      return true;
    }
  }
  return false;
}

bool isOlder(const QJsonObject &candidate, const QJsonObject &current) {
  const auto revision = candidate.value("revision").toInteger(-1);
  const auto currentRevision = current.value("revision").toInteger(-1);
  return revision < currentRevision ||
         (revision == currentRevision &&
          candidate.value("player_revision").toInteger(-1) <
              current.value("player_revision").toInteger(-1));
}

QJsonObject preserveDetail(const QJsonObject &current,
                           const QJsonObject &candidate) {
  if (current.value("revision").toInteger(-1) !=
          candidate.value("revision").toInteger(-2) ||
      !hasMemberSnapshots(current) || hasMemberSnapshots(candidate)) {
    return candidate;
  }
  QJsonObject merged = current;
  for (auto it = candidate.constBegin(); it != candidate.constEnd(); ++it) {
    if (it.key() == "members") {
      continue;
    }
    merged.insert(it.key(), it.value());
  }
  return merged;
}
} // namespace

BuildGroupModel::BuildGroupModel(QObject *parent) : QAbstractListModel(parent) {}

int BuildGroupModel::rowCount(const QModelIndex &parent) const {
  return parent.isValid() ? 0 : groups_.size();
}

QVariant BuildGroupModel::data(const QModelIndex &index, int role) const {
  if (!index.isValid() || index.row() < 0 || index.row() >= groups_.size()) {
    return {};
  }
  const QJsonObject group = groups_.at(index.row()).toObject();
  switch (role) {
  case Qt::DisplayRole:
  case NameRole:
    return group.value("name").toString();
  case IdRole:
    return group.value("id").toString();
  case DefinitionIdRole:
    return group.value("definition_id").toString();
  case InstanceIdRole:
    return group.value("instance_id").toString();
  case RevisionRole:
    return group.value("revision").toInteger();
  case MemberCountRole:
    return group.value("member_count").toInt();
  case SourceCountRole:
    return group.value("source_member_count").toInt();
  case ConfigCountRole:
    return group.value("config_member_count").toInt();
  case UpdatedAtRole:
    return group.value("updated_at").toInteger();
  case MembersRole:
    return group.value("members").toArray().toVariantList();
  case BaselineRole:
    return group.value("baseline").toObject().toVariantMap();
  case RawRole:
    return group.toVariantMap();
  default:
    return {};
  }
}

QHash<int, QByteArray> BuildGroupModel::roleNames() const {
  return {{IdRole, "id"},
          {NameRole, "name"},
          {DefinitionIdRole, "definitionId"},
          {InstanceIdRole, "instanceId"},
          {RevisionRole, "revision"},
          {MemberCountRole, "memberCount"},
          {SourceCountRole, "sourceCount"},
          {ConfigCountRole, "configCount"},
          {UpdatedAtRole, "updatedAt"},
          {MembersRole, "members"},
          {BaselineRole, "baseline"},
          {RawRole, "raw"}};
}

bool BuildGroupModel::replace(const QJsonObject &data, QString *error) {
  if (!data.value("groups").isArray()) {
    if (error) {
      *error = "daemon returned malformed build group list";
    }
    return false;
  }
  QJsonArray next;
  for (const QJsonValue &value : data.value("groups").toArray()) {
    if (!value.isObject() || !valid(value.toObject())) {
      if (error) {
        *error = "build group is missing identity";
      }
      return false;
    }
    QJsonObject candidate = value.toObject();
    const QJsonObject current = group(candidate.value("id").toString());
    if (isOlder(candidate, current)) {
      candidate = current;
    } else {
      candidate = preserveDetail(current, candidate);
    }
    next.append(candidate);
  }
  beginResetModel();
  groups_ = next;
  sortGroups();
  endResetModel();
  return true;
}

bool BuildGroupModel::upsert(const QJsonObject &group, QString *error) {
  if (!valid(group)) {
    if (error) {
      *error = "build group is missing identity";
    }
    return false;
  }
  const QString id = group.value("id").toString();
  const QJsonObject current = this->group(id);
  if (isOlder(group, current)) {
    return true;
  }
  const QJsonObject candidate = preserveDetail(current, group);
  beginResetModel();
  bool found = false;
  for (qsizetype index = 0; index < groups_.size(); ++index) {
    if (groups_.at(index).toObject().value("id").toString() == id) {
      groups_[index] = candidate;
      found = true;
      break;
    }
  }
  if (!found) {
    groups_.append(candidate);
  }
  sortGroups();
  endResetModel();
  return true;
}

void BuildGroupModel::remove(const QString &id) {
  for (qsizetype index = 0; index < groups_.size(); ++index) {
    if (groups_.at(index).toObject().value("id").toString() != id) {
      continue;
    }
    beginRemoveRows({}, index, index);
    groups_.removeAt(index);
    endRemoveRows();
    return;
  }
}

void BuildGroupModel::clear() {
  beginResetModel();
  groups_ = {};
  endResetModel();
}

QJsonObject BuildGroupModel::group(const QString &id) const {
  for (const QJsonValue &value : groups_) {
    const QJsonObject group = value.toObject();
    if (group.value("id").toString() == id) {
      return group;
    }
  }
  return {};
}

bool BuildGroupModel::valid(const QJsonObject &group) {
  return !group.value("id").toString().isEmpty() &&
         !group.value("name").toString().isEmpty() &&
         !group.value("definition_id").toString().isEmpty() &&
         group.value("revision").isDouble();
}

void BuildGroupModel::sortGroups() {
  QList<QJsonObject> sorted;
  sorted.reserve(groups_.size());
  for (const QJsonValue &value : groups_) {
    sorted.append(value.toObject());
  }
  std::sort(sorted.begin(), sorted.end(), groupBefore);
  groups_ = {};
  for (const QJsonObject &group : sorted) {
    groups_.append(group);
  }
}
