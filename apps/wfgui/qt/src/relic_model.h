#pragma once

#include <QAbstractListModel>
#include <QHash>
#include <QJsonObject>
#include <QSet>
#include <QSortFilterProxyModel>

#include <memory>

#include "asset_ref.h"

class RelicModel final : public QAbstractListModel {
  Q_OBJECT

public:
  enum Role {
    NameRole = Qt::UserRole + 1,
    AmountOwnedRole,
    VaultedRole,
    FavoriteRole,
    HasPriceRole,
    ExpectedPlatinumRole,
    ExpectedDucatsRole,
    PricesLoadingRole,
    RefinementRole,
    RelicImageRole,
    PriceCompleteRole,
    RefinementsRole,
    RewardsRole,
    EraRole,
    RelicAssetRole,
  };

  explicit RelicModel(QObject *parent = nullptr);
  ~RelicModel() override;

  int rowCount(const QModelIndex &parent = {}) const override;
  QVariant data(const QModelIndex &index, int role) const override;
  QHash<int, QByteArray> roleNames() const override;

  bool replace(const QJsonObject &data, QString *error = nullptr);
  void clear();
  void setPricesLoading(bool loading);
  void setAssets(const wfgui::AssetMap &assets);
  void applyAssets(const wfgui::AssetMap &assets);
  int traceCount() const;

private:
  void rebuildAssetIndex();
  void notifyAssetRows(const QSet<int> &rows);

  struct Storage;
  std::unique_ptr<Storage> storage_;
  wfgui::AssetMap assets_;
  QMultiHash<QString, int> assetRows_;
  bool pricesLoading_ = false;
};

class RelicFilterModel final : public QSortFilterProxyModel {
  Q_OBJECT

public:
  explicit RelicFilterModel(QObject *parent = nullptr);

  QString filterText() const;
  bool onlyOwned() const;
  QString era() const;
  QVariant data(const QModelIndex &index,
                int role = Qt::DisplayRole) const override;
  void setFilterText(const QString &text);
  void setOnlyOwned(bool onlyOwned);
  void setEra(const QString &era);

signals:
  void filterTextChanged();
  void onlyOwnedChanged();
  void eraChanged();

protected:
  bool filterAcceptsRow(int sourceRow,
                        const QModelIndex &sourceParent) const override;

private:
  QString filterText_;
  QString era_ = "all";
  bool onlyOwned_ = true;
};
