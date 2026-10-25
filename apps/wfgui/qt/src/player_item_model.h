#pragma once

#include <QAbstractListModel>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QSet>
#include <QSortFilterProxyModel>
#include <QString>
#include <QStringList>
#include <QVariantList>

#include <optional>

#include "asset_ref.h"

class PlayerItemModel final : public QAbstractListModel {
  Q_OBJECT

public:
  enum Role {
    NameRole = Qt::UserRole + 1,
    IdRole,
    GroupRole,
    CategoryRole,
    TypeRole,
    QuantityRole,
    DucatsRole,
    MasteredRole,
    AssetPathRole,
    OwnedRole,
    PendingRole,
    RankRole,
    MaxRankRole,
    PotentialXpRole,
    MissingPartsRole,
    FromRelicsRole,
    RelicProbabilityRole,
    BuyableRole,
    AcquisitionPlatinumRole,
    AcquisitionPriceStateRole,
    HasRecipeRole,
    ComponentsRole,
    AssetSpecRole,
    TradableRole,
    MarketNameRole,
    SellableRole,
    PlatinumRole,
    BuyPlatinumRole,
    PriceStateRole,
    IsPrimeRole,
    MasteryRequirementRole,
    ReadyToBuildRole,
    FavoriteRole,
    VaultedRole,
    SubsumedRole,
    AssetRefRole,
  };

  explicit PlayerItemModel(QObject *parent = nullptr);

  int rowCount(const QModelIndex &parent = {}) const override;
  QVariant data(const QModelIndex &index, int role) const override;
  int ownedQuantity(const QString &name,
                    std::optional<int> rank = std::nullopt) const;
  bool setData(const QModelIndex &index, const QVariant &value,
               int role) override;
  QHash<int, QByteArray> roleNames() const override;

  bool replace(const QJsonObject &data, QString *error = nullptr);
  void clear();
  void setAssets(const wfgui::AssetMap &assets);
  void applyAssets(const wfgui::AssetMap &assets);
  void applyMarketQuotes(const QJsonArray &quotes, const QJsonArray &missing);
  void markMarketUnavailable(const QStringList &items);

private:
  void rebuildIndexes();
  void notifyRows(const QSet<int> &rows, const QList<int> &roles);
  void notifyMarketRows(const QSet<QString> &names);

  QList<QJsonObject> items_;
  wfgui::AssetMap assets_;
  QMultiHash<QString, int> assetRows_;
  QMultiHash<QString, int> nameRows_;
  mutable QHash<int, QVariantList> componentCache_;
  QHash<QString, QJsonObject> marketQuotes_;
  QSet<QString> unavailableMarketItems_;
  QHash<QString, bool> favoriteOverrides_;
};

class PlayerItemFilterModel final : public QSortFilterProxyModel {
  Q_OBJECT

public:
  explicit PlayerItemFilterModel(QObject *parent = nullptr);

  QVariant data(const QModelIndex &index,
                int role = Qt::DisplayRole) const override;
  void setText(const QString &text);
  void setGroup(const QString &group);
  void setMode(const QString &mode);
  void setPricesLoading(bool loading);
  void setFlag(const QString &name, int state);
  void setSortMode(const QString &mode);
  void setSortAscending(bool ascending);

protected:
  bool filterAcceptsRow(int sourceRow,
                        const QModelIndex &sourceParent) const override;
  bool lessThan(const QModelIndex &left,
                const QModelIndex &right) const override;

private:
  QString text_;
  QString group_ = "all";
  QString mode_ = "all";
  QString sortMode_ = "name";
  bool sortAscending_ = true;
  bool pricesLoading_ = false;
  QHash<QString, int> flags_;
};
