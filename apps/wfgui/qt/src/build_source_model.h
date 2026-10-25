#pragma once

#include <QAbstractListModel>
#include <QJsonArray>
#include <QJsonObject>

class BuildItemModel final : public QAbstractListModel {
  Q_OBJECT

public:
  enum Role {
    ExternalIdRole = Qt::UserRole + 1,
    CanonicalIdRole,
    NameRole,
    ClassRole,
    CategoriesRole,
    TextureRole,
    RawRole,
  };

  explicit BuildItemModel(QObject *parent = nullptr);
  int rowCount(const QModelIndex &parent = QModelIndex()) const override;
  QVariant data(const QModelIndex &index,
                int role = Qt::DisplayRole) const override;
  QHash<int, QByteArray> roleNames() const override;
  bool replace(const QJsonObject &snapshot, QString *error = nullptr);
  void clear();

private:
  QJsonArray items_;
};

class BuildSummaryModel final : public QAbstractListModel {
  Q_OBJECT

public:
  enum Role {
    ExternalIdRole = Qt::UserRole + 1,
    TitleRole,
    AuthorRole,
    ScoreRole,
    FormasRole,
    UpdatedAtRole,
    ItemRole,
    RawRole,
  };

  explicit BuildSummaryModel(QObject *parent = nullptr);
  int rowCount(const QModelIndex &parent = QModelIndex()) const override;
  QVariant data(const QModelIndex &index,
                int role = Qt::DisplayRole) const override;
  QHash<int, QByteArray> roleNames() const override;
  bool replace(const QJsonObject &snapshot, QString *error = nullptr);
  void clear();

private:
  QJsonArray builds_;
};
