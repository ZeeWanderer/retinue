#pragma once

#include <QAbstractListModel>
#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QSortFilterProxyModel>
#include <QString>

#include "asset_ref.h"

class BuildEquipmentModel final : public QAbstractListModel {
  Q_OBJECT

public:
  enum Role {
    IdRole = Qt::UserRole + 1,
    NameRole,
    ClassRole,
    CategoryRole,
    AssetSpecRole,
    AssetPathRole,
    InstancesRole,
    InstanceCountRole,
  };

  explicit BuildEquipmentModel(QObject *parent = nullptr);

  int rowCount(const QModelIndex &parent = {}) const override;
  QVariant data(const QModelIndex &index, int role) const override;
  QHash<int, QByteArray> roleNames() const override;

  bool replace(const QJsonObject &data, QString *error = nullptr);
  void clear();
  void setAssets(const wfgui::AssetMap &assets);
  void applyAssets(const wfgui::AssetMap &assets);

  qint64 revision() const;
  qint64 updatedAt() const;

private:
  struct Group {
    QJsonObject definition;
    QJsonArray instances;
  };

  QList<Group> groups_;
  wfgui::AssetMap assets_;
  qint64 revision_ = -1;
  qint64 updatedAt_ = 0;
};

class BuildEquipmentFilterModel final : public QSortFilterProxyModel {
  Q_OBJECT

public:
  explicit BuildEquipmentFilterModel(QObject *parent = nullptr);

  void setText(const QString &text);
  void setCategory(const QString &category);

protected:
  bool filterAcceptsRow(int sourceRow,
                        const QModelIndex &sourceParent) const override;
  bool lessThan(const QModelIndex &left,
                const QModelIndex &right) const override;

private:
  QString text_;
  QString category_ = "all";
};
