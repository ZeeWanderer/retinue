#include "build_source_model.h"

#include <QJsonValue>

namespace {
bool validateArray(const QJsonObject &snapshot, const QString &key,
                   const QStringList &required, QString *error) {
  const QJsonValue value = snapshot.value(key);
  if (!value.isArray()) {
    if (error) {
      *error = QString("build source response has no %1 array").arg(key);
    }
    return false;
  }
  for (const QJsonValue &entry : value.toArray()) {
    if (!entry.isObject()) {
      if (error) {
        *error = QString("build source %1 contains a non-object").arg(key);
      }
      return false;
    }
    const QJsonObject object = entry.toObject();
    for (const QString &field : required) {
      if (!object.contains(field)) {
        if (error) {
          *error = QString("build source %1 entry has no %2").arg(key, field);
        }
        return false;
      }
    }
  }
  return true;
}

QJsonObject objectAt(const QJsonArray &values, const QModelIndex &index) {
  if (!index.isValid() || index.row() < 0 || index.row() >= values.size()) {
    return {};
  }
  return values.at(index.row()).toObject();
}
} // namespace

BuildItemModel::BuildItemModel(QObject *parent) : QAbstractListModel(parent) {}

int BuildItemModel::rowCount(const QModelIndex &parent) const {
  return parent.isValid() ? 0 : items_.size();
}

QVariant BuildItemModel::data(const QModelIndex &index, int role) const {
  const QJsonObject item = objectAt(items_, index);
  if (item.isEmpty()) {
    return {};
  }
  switch (role) {
  case Qt::DisplayRole:
  case NameRole:
    return item.value("name").toString();
  case Qt::ToolTipRole:
  case CanonicalIdRole:
    return item.value("canonical_id").toString();
  case ExternalIdRole:
    return item.value("external_id").toInteger();
  case ClassRole:
    return item.value("class").toString();
  case CategoriesRole:
    return item.value("categories").toArray().toVariantList();
  case TextureRole:
    return item.value("texture").toString();
  case RawRole:
    return item.toVariantMap();
  default:
    return {};
  }
}

QHash<int, QByteArray> BuildItemModel::roleNames() const {
  return {{ExternalIdRole, "externalId"}, {CanonicalIdRole, "canonicalId"},
          {NameRole, "name"},             {ClassRole, "class"},
          {CategoriesRole, "categories"}, {TextureRole, "texture"},
          {RawRole, "raw"}};
}

bool BuildItemModel::replace(const QJsonObject &snapshot, QString *error) {
  if (!validateArray(snapshot, "items", {"canonical_id", "name"}, error)) {
    return false;
  }
  beginResetModel();
  items_ = snapshot.value("items").toArray();
  endResetModel();
  return true;
}

void BuildItemModel::clear() {
  beginResetModel();
  items_ = {};
  endResetModel();
}

BuildSummaryModel::BuildSummaryModel(QObject *parent)
    : QAbstractListModel(parent) {}

int BuildSummaryModel::rowCount(const QModelIndex &parent) const {
  return parent.isValid() ? 0 : builds_.size();
}

QVariant BuildSummaryModel::data(const QModelIndex &index, int role) const {
  const QJsonObject build = objectAt(builds_, index);
  if (build.isEmpty()) {
    return {};
  }
  const QJsonObject identity = build.value("identity").toObject();
  const QJsonObject author = build.value("author").toObject();
  switch (role) {
  case Qt::DisplayRole:
  case TitleRole:
    return build.value("title").toString();
  case Qt::ToolTipRole:
    return QString("%1 · %2 Forma · score %3")
        .arg(author.value("username").toString("Unknown"))
        .arg(build.value("formas").toInt())
        .arg(build.value("score").toInt());
  case ExternalIdRole:
    return identity.value("external_id").toInteger();
  case AuthorRole:
    return author.value("username").toString();
  case ScoreRole:
    return build.value("score").toInt();
  case FormasRole:
    return build.value("formas").toInt();
  case UpdatedAtRole:
    return build.value("updated_at").toString();
  case ItemRole:
    return build.value("item").toObject().toVariantMap();
  case RawRole:
    return build.toVariantMap();
  default:
    return {};
  }
}

QHash<int, QByteArray> BuildSummaryModel::roleNames() const {
  return {{ExternalIdRole, "externalId"}, {TitleRole, "title"},
          {AuthorRole, "author"},         {ScoreRole, "score"},
          {FormasRole, "formas"},         {UpdatedAtRole, "updatedAt"},
          {ItemRole, "item"},             {RawRole, "raw"}};
}

bool BuildSummaryModel::replace(const QJsonObject &snapshot, QString *error) {
  if (!validateArray(snapshot, "builds", {"identity", "title"}, error)) {
    return false;
  }
  beginResetModel();
  builds_ = snapshot.value("builds").toArray();
  endResetModel();
  return true;
}

void BuildSummaryModel::clear() {
  beginResetModel();
  builds_ = {};
  endResetModel();
}
