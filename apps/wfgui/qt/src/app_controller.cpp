#include "app_controller.h"

#include <QDateTime>
#include <QJsonDocument>
#include <QJsonObject>
#include <QPointer>
#include <QSet>
#include <QTimer>

#include "image_cache.h"

namespace {
constexpr int AssetRefreshIntervalMs = 30 * 60'000;

QString assetRequestIdentity(const QJsonObject &spec) {
  return spec.value("source").toString("wfcd") + QChar::Null +
         spec.value("image_name").toString();
}

void appendAsset(const QJsonObject &asset, QJsonArray &assets,
                 QSet<QString> &ids) {
  const QString id = asset.value("id").toString();
  if (!id.isEmpty() && !ids.contains(id)) {
    ids.insert(id);
    assets.append(asset);
  }
}

QJsonArray buildAssets(const QJsonObject &data) {
  QJsonArray assets;
  QSet<QString> ids;
  for (const QJsonValue &value : data.value("definitions").toArray()) {
    appendAsset(value.toObject().value("asset").toObject(), assets, ids);
  }
  for (const QJsonValue &instanceValue : data.value("instances").toArray()) {
    const QJsonObject instance = instanceValue.toObject();
    for (const QJsonValue &configValue : instance.value("configs").toArray()) {
      for (const QJsonValue &upgradeValue :
           configValue.toObject().value("upgrade_slots").toArray()) {
        appendAsset(upgradeValue.toObject().value("asset").toObject(), assets,
                    ids);
      }
    }
  }
  return assets;
}
} // namespace

AppController::AppController(QObject *parent)
    : QObject(parent), daemon_(this), relics_(this), filteredRelics_(this),
      inventoryItems_(this), masteryItems_(this), foundryItems_(this),
      buildEquipment_(this), buildGroups_(this), buildSourceItems_(this),
      sourceBuilds_(this) {
  filteredRelics_.setSourceModel(&relics_);
  assets_.insert("builtin:forma", wfgui::AssetRef::embedded(
                                      "builtin:forma", ":/assets/forma.png"));
  const QPointer<DaemonClient> daemon(&daemon_);
  wfgui::setImageIssueReporter(
      [daemon](const wfgui::AssetRef &asset, const QString &reason,
               bool resolved) {
        if (!daemon) {
          return;
        }
        if (resolved) {
          daemon->clearResolutionIssue("asset_decode", asset.id);
        } else {
          const QString fallback =
              asset.imageName.isEmpty() ? asset.path : asset.imageName;
          daemon->setResolutionIssue("asset_decode", asset.id, reason,
                                     fallback);
        }
      });

  connect(&daemon_, &DaemonClient::connectionChanged, this,
          &AppController::connectedChanged);
  connect(&daemon_, &DaemonClient::statusChanged, this,
          &AppController::statusChanged);
  connect(&daemon_, &DaemonClient::marketQuoteCacheSettled, this,
          &AppController::marketQuoteCacheSettled);
  connect(&daemon_, &DaemonClient::marketQuoteFetchSettled, this,
          &AppController::marketQuoteFetchSettled);
  connect(&filteredRelics_, &RelicFilterModel::filterTextChanged, this,
          &AppController::filterTextChanged);
  connect(&filteredRelics_, &RelicFilterModel::onlyOwnedChanged, this,
          &AppController::onlyOwnedChanged);
  connect(&daemon_, &DaemonClient::relicPlannerReady, this,
          [this](const QString &era, bool prices, const QJsonObject &data) {
            if (era != "all") {
              return;
            }
            EraState &state = relicState_;
            const qint64 revision = data.value("revision").toInteger(-1);
            if (playerRevision_ >= 0 && revision >= 0 &&
                revision < playerRevision_) {
              if (prices) {
                state.pricesPending = false;
              } else {
                state.metadataPending = false;
              }
              state.stale = true;
              if (activePage_ == "relic") {
                refresh();
              }
              return;
            }
            if (prices) {
              state.priced = data;
              state.hasPrices = true;
              state.pricesPending = false;
              state.pricesRevision = revision;
            } else {
              state.metadata = data;
              state.hasMetadata = true;
              state.metadataPending = false;
              state.metadataRevision = revision;
            }
            const bool metadataCurrent =
                !state.hasMetadata || playerRevision_ < 0 ||
                state.metadataRevision >= playerRevision_;
            const bool pricesCurrent = !state.hasPrices ||
                                       playerRevision_ < 0 ||
                                       state.pricesRevision >= playerRevision_;
            state.stale = !metadataCurrent || !pricesCurrent;
            state.error.clear();
            requestAssets(data);
            applySelectedEra();
          });
  connect(&daemon_, &DaemonClient::assetsResolved, this,
          &AppController::applyAssets);
  connect(&daemon_, &DaemonClient::assetRefreshed, this,
          &AppController::applyAssetRefresh);
  connect(&daemon_, &DaemonClient::assetRequestFailed, this,
          [this] { assetRequestedAt_.clear(); });
  connect(&daemon_, &DaemonClient::assetCacheStatusReady, this,
          [this](const QJsonObject &status) {
            sourceAssetCache_ = status;
            if (status.value("objects").toInteger() == 0) {
              assetRequestedAt_.clear();
            }
            sourceAssetCacheError_.clear();
            sourceAssetCacheBusy_ = false;
            emit sourceAssetCacheChanged();
          });
  connect(&daemon_, &DaemonClient::assetCacheRequestFailed, this,
          [this](const QString &requestError) {
            sourceAssetCacheError_ = requestError;
            sourceAssetCacheBusy_ = false;
            emit sourceAssetCacheChanged();
          });
  connect(&daemon_, &DaemonClient::requestFailed, this,
          [this](const QString &era, bool prices, const QString &requestError) {
            if (era != "all") {
              return;
            }
            EraState &state = relicState_;
            state.error = requestError;
            if (prices) {
              state.pricesPending = false;
            } else {
              state.metadataPending = false;
            }
            applySelectedEra();
          });
  connect(&daemon_, &DaemonClient::playerViewReady, this,
          &AppController::applyPlayerView);
  connect(&daemon_, &DaemonClient::playerDatasetChanged, this,
          &AppController::handlePlayerDatasetChanged);
  connect(&daemon_, &DaemonClient::playerViewFailed, this,
          [this](const QString &view, const QString &requestError) {
            if (PlayerViewState *state = playerState(view)) {
              state->pending = false;
              state->error = requestError;
              emitPlayerStateChanged(view);
            }
          });
  connect(&daemon_, &DaemonClient::activityReady, this,
          [this](const QJsonObject &data) {
            activity_ = data;
            activityError_.clear();
            emit activityStateChanged();
          });
  connect(&daemon_, &DaemonClient::activityFailed, this,
          [this](const QString &requestError) {
            activityError_ = requestError;
            emit activityStateChanged();
          });
  connect(
      &daemon_, &DaemonClient::notificationSettingsReady, this,
      [this](const QJsonObject &settings) {
        const QString mode =
            settings.value("fissures").toObject().value("mode").toString("off");
        if (mode != "off" && mode != "session" && mode != "persistent") {
          return;
        }
        const bool changed =
            fissureNotificationMode_ != mode || !notificationSettingsLoaded_;
        fissureNotificationMode_ = mode;
        notificationSettingsLoaded_ = true;
        if (changed) {
          emit notificationSettingsChanged();
        }
      });
  connect(&daemon_, &DaemonClient::marketQuotesResolved, this,
          [this](const QJsonArray &quotes, const QJsonArray &missing) {
            foundryItems_.applyMarketQuotes(quotes, missing);
            inventoryItems_.applyMarketQuotes(quotes, missing);
            masteryItems_.applyMarketQuotes(quotes, missing);
            QJsonArray descriptors;
            for (const QJsonValue &value : quotes) {
              const QJsonObject row = value.toObject();
              const QJsonObject item = row.value("item_data").toObject();
              if (!item.isEmpty()) {
                descriptors.append(item);
              }
              const QStringList keys = {
                  row.value("item").toString(), row.value("slug").toString(),
                  item.value("id").toString(), item.value("slug").toString(),
                  item.value("name").toString()};
              for (const QString &key : keys) {
                if (!key.isEmpty()) {
                  marketQuotes_.insert(marketKey(key), row);
                }
              }
            }
            applyMarketDescriptors(descriptors);
            emit marketQuotesChanged();
          });
  connect(&daemon_, &DaemonClient::marketQuoteRequestFailed, this,
          [this](const QStringList &items, const QString &) {
            for (const QString &item : items) {
              marketRequestedAt_.remove(item);
            }
            foundryItems_.markMarketUnavailable(items);
            inventoryItems_.markMarketUnavailable(items);
            masteryItems_.markMarketUnavailable(items);
            emit marketQuotesChanged();
          });
  connect(&daemon_, &DaemonClient::marketVariantQuoteReady, this,
          [this](const QString &item, const QJsonObject &filters,
                 const QJsonObject &data) {
            const QString key = marketVariantKey(item, filters);
            marketVariantPending_.remove(key);
            marketVariantQuotes_.insert(key, data);
            emit marketVariantQuoteReady(item, filters, data);
          });
  connect(&daemon_, &DaemonClient::marketVariantQuoteFailed, this,
          [this](const QString &item, const QJsonObject &filters,
                 const QString &error) {
            marketVariantPending_.remove(marketVariantKey(item, filters));
            emit marketVariantQuoteFailed(item, filters, error);
          });
  connect(&daemon_, &DaemonClient::marketMatchesResolved, this,
          &AppController::marketSearchReady);
  connect(&daemon_, &DaemonClient::marketMatchesFailed, this,
          &AppController::marketSearchFailed);
  connect(&daemon_, &DaemonClient::marketItemsDescribed, this,
          [this](const QJsonArray &items, const QJsonArray &) {
            applyMarketDescriptors(items);
          });
  connect(&daemon_, &DaemonClient::marketAccountReady, this,
          [this](const QString &action, const QJsonObject &account) {
            if (action == "snapshot") {
              marketPending_ = false;
            } else {
              finishMarketAction();
            }
            marketAccount_ = account;
            marketLoaded_ = true;
            marketError_.clear();
            QStringList items;
            for (const QJsonValue &value : account.value("orders").toArray()) {
              const QString item = value.toObject().value("itemId").toString();
              if (!item.isEmpty() && !items.contains(item)) {
                items.append(item);
              }
            }
            describeMarketItems(items);
            resolveMarketQuotes(items);
            emit marketAccountChanged();
          });
  connect(&daemon_, &DaemonClient::marketAccountFailed, this,
          [this](const QString &action, const QString &requestError) {
            if (action == "snapshot") {
              marketPending_ = false;
            } else {
              finishMarketAction();
            }
            marketError_ = requestError;
            if (requestError.contains("session expired", Qt::CaseInsensitive)) {
              marketAccount_ = {{"authenticated", false},
                                {"profile", QJsonValue::Null},
                                {"orders", QJsonArray{}}};
              marketLoaded_ = true;
            }
            emit marketAccountChanged();
          });
  connect(&daemon_, &DaemonClient::marketPresenceReady, this,
          [this](const QJsonObject &presence, bool requested) {
            if (requested) {
              finishMarketAction();
            }
            marketAccount_.insert("presence", presence);
            marketError_.clear();
            emit marketAccountChanged();
          });
  connect(&daemon_, &DaemonClient::marketPresenceFailed, this,
          [this](const QString &requestError) {
            finishMarketAction();
            marketError_ = requestError;
            emit marketAccountChanged();
          });
  connect(&daemon_, &DaemonClient::overframeAccountReady, this,
          [this](const QString &, const QJsonObject &account) {
            overframeAccount_ = account;
            overframeAccountError_.clear();
            overframeAccountPending_ = false;
            emit overframeAccountChanged();
          });
  connect(&daemon_, &DaemonClient::overframeAccountFailed, this,
          [this](const QString &, const QString &requestError) {
            overframeAccountError_ = requestError;
            overframeAccountPending_ = false;
            emit overframeAccountChanged();
          });
  connect(&daemon_, &DaemonClient::buildSourceReady, this,
          &AppController::applyBuildSourceReply);
  connect(&daemon_, &DaemonClient::buildSourceFailed, this,
          &AppController::applyBuildSourceError);
  connect(&daemon_, &DaemonClient::buildGroupEvent, this,
          [this](const QString &action, const QJsonObject &group) {
            const QString id = group.value("id").toString();
            if (action == "deleted") {
              buildGroups_.remove(id);
            } else {
              QString parseError;
              if (!buildGroups_.upsert(group, &parseError)) {
                buildGroupsError_ = parseError;
              }
            }
            const qint64 revision = group.value("player_revision").toInteger(-1);
            if (revision >= 0 && revision < playerRevision_) {
              buildGroupsLoaded_ = false;
              if (activePage_ == "build-planner") {
                refreshBuildGroups();
              }
            }
            emit buildGroupsStateChanged();
            emit buildGroupChanged(action, group);
          });

  daemon_.start();
  ensureFoundry();
  refreshActivity();
  auto *activityTimer = new QTimer(this);
  activityTimer->setInterval(60'000);
  connect(activityTimer, &QTimer::timeout, this,
          &AppController::refreshActivity);
  activityTimer->start();

  auto *assetRefreshTimer = new QTimer(this);
  assetRefreshTimer->setInterval(AssetRefreshIntervalMs);
  connect(assetRefreshTimer, &QTimer::timeout, this, [this] {
    QJsonArray specs;
    for (auto asset = assets_.cbegin(); asset != assets_.cend(); ++asset) {
      if (!asset->isPersistent()) {
        continue;
      }
      specs.append(QJsonObject{{"id", asset->id},
                               {"source", asset->source},
                               {"image_name", asset->imageName}});
    }
    resolveAssets(specs);
  });
  assetRefreshTimer->start();
}

QAbstractItemModel *AppController::relics() { return &filteredRelics_; }

QAbstractItemModel *AppController::foundryItems() { return &foundryItems_; }

QAbstractItemModel *AppController::inventoryItems() { return &inventoryItems_; }

QAbstractItemModel *AppController::masteryItems() { return &masteryItems_; }

QAbstractItemModel *AppController::buildEquipment() { return &buildEquipment_; }

QAbstractItemModel *AppController::buildGroups() { return &buildGroups_; }

QAbstractItemModel *AppController::buildSourceItems() {
  return &buildSourceItems_;
}

QAbstractItemModel *AppController::sourceBuilds() { return &sourceBuilds_; }

QString AppController::selectedEra() const { return selectedEra_; }

QString AppController::filterText() const {
  return filteredRelics_.filterText();
}

bool AppController::onlyOwned() const { return filteredRelics_.onlyOwned(); }

QString AppController::status() const { return daemon_.status(); }

QString AppController::error() const { return error_; }

bool AppController::connected() const { return daemon_.connected(); }

bool AppController::loading() const { return loading_; }

bool AppController::pricing() const { return relicState_.pricesPending; }

int AppController::traceCount() const { return relics_.traceCount(); }

QJsonObject AppController::foundrySummary() const {
  return foundryState_.summary;
}

QJsonObject AppController::inventorySummary() const {
  return inventoryState_.summary;
}

QJsonObject AppController::masterySummary() const {
  return masteryState_.summary;
}

QJsonObject AppController::playerProfile() const { return playerProfile_; }

QString AppController::assetPath(const QString &id) const {
  return assets_.value(id).path;
}

wfgui::AssetRef AppController::assetRef(const QString &id) const {
  return assets_.value(id);
}

QJsonObject AppController::activity() const {
  QJsonObject result = activity_;
  if (!activityError_.isEmpty()) {
    result.insert("error", activityError_);
  }
  return result;
}

QString AppController::fissureNotificationMode() const {
  return fissureNotificationMode_;
}

bool AppController::notificationSettingsLoaded() const {
  return notificationSettingsLoaded_;
}

QString AppController::foundryError() const { return foundryState_.error; }

QString AppController::inventoryError() const { return inventoryState_.error; }

QString AppController::masteryError() const { return masteryState_.error; }

bool AppController::foundryLoading() const { return foundryState_.pending; }

bool AppController::inventoryLoading() const { return inventoryState_.pending; }

bool AppController::masteryLoading() const { return masteryState_.pending; }

bool AppController::foundryLoaded() const { return foundryState_.loaded; }

bool AppController::inventoryLoaded() const { return inventoryState_.loaded; }

bool AppController::masteryLoaded() const { return masteryState_.loaded; }

QString AppController::buildEquipmentError() const {
  return buildEquipmentState_.error;
}

bool AppController::buildEquipmentLoading() const {
  return buildEquipmentState_.pending;
}

bool AppController::buildEquipmentLoaded() const {
  return buildEquipmentState_.loaded;
}

QString AppController::buildGroupsError() const { return buildGroupsError_; }

bool AppController::buildGroupsLoading() const {
  return !pendingBuildGroupRequests_.isEmpty();
}

bool AppController::buildGroupsLoaded() const { return buildGroupsLoaded_; }

QJsonObject AppController::buildGroup(const QString &id) const {
  return buildGroups_.group(id);
}

QString AppController::buildSourceItemsError() const {
  return buildSourceItemsError_;
}

QString AppController::sourceBuildsError() const { return sourceBuildsError_; }

bool AppController::buildSourceItemsLoading() const {
  return !currentBuildItemRequest_.isEmpty() &&
         pendingBuildRequests_.contains(currentBuildItemRequest_);
}

bool AppController::sourceBuildsLoading() const {
  return !currentBuildListRequest_.isEmpty() &&
         pendingBuildRequests_.contains(currentBuildListRequest_);
}

QJsonObject AppController::buildRevision(qint64 id) const {
  return buildRevisions_.value(id);
}

QString AppController::buildRevisionError(qint64 id) const {
  return buildRevisionErrors_.value(id);
}

bool AppController::buildRevisionLoading(qint64 id) const {
  const QJsonObject request{{"op", "build_detail"}, {"build_id", id}};
  return pendingBuildRequests_.contains(buildRequestKey(request));
}

QJsonObject AppController::marketAccount() const { return marketAccount_; }

QJsonObject AppController::marketItem(const QString &key) const {
  return marketItems_.value(marketKey(key));
}

QJsonObject AppController::marketQuote(const QString &key) const {
  return marketQuotes_.value(marketKey(key));
}

QJsonObject
AppController::marketVariantQuote(const QString &item,
                                  const QJsonObject &filters) const {
  return marketVariantQuotes_.value(marketVariantKey(item, filters));
}

QString AppController::marketError() const { return marketError_; }

bool AppController::marketLoaded() const { return marketLoaded_; }

bool AppController::marketBusy() const {
  return marketPending_ || marketActions_ > 0;
}

bool AppController::marketQuoteFetchBusy() const {
  return daemon_.marketQuoteFetchBusy();
}

int AppController::ownedMarketQuantity(const QString &name,
                                       const QJsonObject &filters) const {
  if (!inventoryState_.loaded) {
    return -1;
  }
  const std::optional<int> rank = filters.value("rank").isDouble()
                                      ? std::optional(filters.value("rank").toInt())
                                      : std::nullopt;
  return inventoryItems_.ownedQuantity(name, rank);
}

QJsonObject AppController::overframeAccount() const {
  return overframeAccount_;
}

QString AppController::overframeAccountError() const {
  return overframeAccountError_;
}

bool AppController::overframeAccountBusy() const {
  return overframeAccountPending_;
}

QJsonObject AppController::sourceAssetCache() const {
  return sourceAssetCache_;
}

QString AppController::sourceAssetCacheError() const {
  return sourceAssetCacheError_;
}

bool AppController::sourceAssetCacheBusy() const {
  return sourceAssetCacheBusy_;
}

void AppController::setFilterText(const QString &text) {
  filteredRelics_.setFilterText(text);
}

void AppController::setOnlyOwned(bool onlyOwned) {
  filteredRelics_.setOnlyOwned(onlyOwned);
}

void AppController::selectEra(const QString &era) {
  static const QSet<QString> eras = {
      "all", "lith", "meso", "neo", "axi", "requiem",
  };
  if (!eras.contains(era) || selectedEra_ == era) {
    return;
  }
  selectedEra_ = era;
  filteredRelics_.setEra(era);
  emit selectedEraChanged();
}

void AppController::setActivePage(const QString &page) { activePage_ = page; }

void AppController::refresh() {
  relicsRequested_ = true;
  EraState &state = relicState_;
  state.metadataPending = true;
  state.pricesPending = true;
  state.error.clear();
  applySelectedEra();
  daemon_.requestRelics("all", false);
  daemon_.requestRelics("all", true);
}

void AppController::ensureRelics() {
  if (!relicsRequested_) {
    refresh();
  } else if (relicState_.stale && !relicState_.metadataPending &&
             !relicState_.pricesPending) {
    refresh();
  } else if (relicState_.hasPrices) {
    requestAssets(relicState_.priced);
  } else if (relicState_.hasMetadata) {
    requestAssets(relicState_.metadata);
  }
}

void AppController::ensureFoundry() {
  if ((!foundryState_.loaded || foundryState_.stale) &&
      !foundryState_.pending) {
    refreshFoundry();
  }
}

void AppController::ensureInventory() {
  if ((!inventoryState_.loaded || inventoryState_.stale) &&
      !inventoryState_.pending) {
    refreshInventory();
  }
}

void AppController::ensureMastery() {
  if ((!masteryState_.loaded || masteryState_.stale) &&
      !masteryState_.pending) {
    refreshMastery();
  }
}

void AppController::ensureBuildEquipment() {
  if ((!buildEquipmentState_.loaded || buildEquipmentState_.stale) &&
      !buildEquipmentState_.pending) {
    refreshBuildEquipment();
  }
}

void AppController::ensureBuildGroups() {
  if (!buildGroupsLoaded_ && pendingBuildGroupRequests_.isEmpty()) {
    refreshBuildGroups();
  }
}

void AppController::refreshFoundry() {
  foundryState_.pending = true;
  foundryState_.error.clear();
  emit foundryStateChanged();
  daemon_.requestPlayerView("foundry");
}

void AppController::refreshInventory() {
  inventoryState_.pending = true;
  inventoryState_.error.clear();
  emit inventoryStateChanged();
  daemon_.requestPlayerView("inventory");
}

void AppController::refreshMastery() {
  masteryState_.pending = true;
  masteryState_.error.clear();
  emit masteryStateChanged();
  daemon_.requestPlayerView("mastery");
}

void AppController::refreshBuildEquipment() {
  buildEquipmentState_.pending = true;
  buildEquipmentState_.error.clear();
  emit buildEquipmentStateChanged();
  daemon_.requestPlayerView("build_equipment");
}

void AppController::refreshBuildGroups() {
  const QJsonObject request{{"op", "build_group_list"}};
  const QString key = buildRequestKey(request);
  if (pendingBuildGroupRequests_.contains(key)) {
    return;
  }
  buildGroupsError_.clear();
  pendingBuildGroupRequests_.insert(key);
  emit buildGroupsStateChanged();
  daemon_.requestBuildGroups();
}

void AppController::requestBuildGroup(const QString &id) {
  if (id.isEmpty()) {
    return;
  }
  const QJsonObject request{{"op", "build_group_get"}, {"group_id", id}};
  const QString key = buildRequestKey(request);
  if (pendingBuildGroupRequests_.contains(key)) {
    return;
  }
  buildGroupsError_.clear();
  pendingBuildGroupRequests_.insert(key);
  emit buildGroupsStateChanged();
  daemon_.requestBuildGroup(id);
}

void AppController::createBuildGroup(const QJsonObject &group) {
  const QJsonObject request{{"op", "build_group_create"}, {"group", group}};
  const QString key = buildRequestKey(request);
  if (pendingBuildGroupRequests_.contains(key)) {
    return;
  }
  buildGroupsError_.clear();
  pendingBuildGroupRequests_.insert(key);
  emit buildGroupsStateChanged();
  daemon_.createBuildGroup(group);
}

void AppController::updateBuildGroup(const QString &id, qint64 revision,
                                     const QJsonObject &patch) {
  const QJsonObject request{{"op", "build_group_update"},
                            {"group_id", id},
                            {"revision", revision},
                            {"patch", patch}};
  const QString key = buildRequestKey(request);
  if (pendingBuildGroupRequests_.contains(key)) {
    return;
  }
  buildGroupsError_.clear();
  pendingBuildGroupRequests_.insert(key);
  emit buildGroupsStateChanged();
  daemon_.updateBuildGroup(id, revision, patch);
}

void AppController::deleteBuildGroup(const QString &id, qint64 revision) {
  const QJsonObject request{{"op", "build_group_delete"},
                            {"group_id", id},
                            {"revision", revision}};
  const QString key = buildRequestKey(request);
  if (pendingBuildGroupRequests_.contains(key)) {
    return;
  }
  buildGroupsError_.clear();
  pendingBuildGroupRequests_.insert(key);
  emit buildGroupsStateChanged();
  daemon_.deleteBuildGroup(id, revision);
}

void AppController::addBuildSourceToGroup(const QString &id, qint64 revision,
                                          qint64 externalId,
                                          const QString &fingerprint) {
  QJsonObject request{{"op", "build_group_add_source"},
                      {"group_id", id},
                      {"revision", revision},
                      {"source", "overframe"},
                      {"external_id", externalId}};
  if (!fingerprint.isEmpty()) {
    request.insert("fingerprint", fingerprint);
  }
  const QString key = buildRequestKey(request);
  if (pendingBuildGroupRequests_.contains(key)) {
    return;
  }
  buildGroupsError_.clear();
  pendingBuildGroupRequests_.insert(key);
  emit buildGroupsStateChanged();
  daemon_.addBuildSourceToGroup(id, revision, externalId, fingerprint);
}

void AppController::addBuildConfigToGroup(const QString &id, qint64 revision,
                                          const QString &instanceId,
                                          int configIndex) {
  const QJsonObject request{{"op", "build_group_add_config"},
                            {"group_id", id},
                            {"revision", revision},
                            {"instance_id", instanceId},
                            {"config_index", configIndex}};
  const QString key = buildRequestKey(request);
  if (pendingBuildGroupRequests_.contains(key)) {
    return;
  }
  buildGroupsError_.clear();
  pendingBuildGroupRequests_.insert(key);
  emit buildGroupsStateChanged();
  daemon_.addBuildConfigToGroup(id, revision, instanceId, configIndex);
}

void AppController::removeBuildGroupMember(const QString &id, qint64 revision,
                                           const QString &memberId) {
  const QJsonObject request{{"op", "build_group_remove_member"},
                            {"group_id", id},
                            {"revision", revision},
                            {"member_id", memberId}};
  const QString key = buildRequestKey(request);
  if (pendingBuildGroupRequests_.contains(key)) {
    return;
  }
  buildGroupsError_.clear();
  pendingBuildGroupRequests_.insert(key);
  emit buildGroupsStateChanged();
  daemon_.removeBuildGroupMember(id, revision, memberId);
}

void AppController::planBuildGroup(const QString &id, qint64 revision) {
  const QJsonObject request{{"op", "build_group_plan"},
                            {"group_id", id},
                            {"revision", revision}};
  const QString key = buildRequestKey(request);
  if (pendingBuildGroupRequests_.contains(key)) {
    return;
  }
  buildGroupsError_.clear();
  pendingBuildGroupRequests_.insert(key);
  emit buildGroupsStateChanged();
  daemon_.planBuildGroup(id, revision);
}

void AppController::searchBuildItems(const QString &query,
                                     const QString &category, int limit) {
  const QJsonObject request{{"op", "build_search"},
                            {"query", query},
                            {"class", category},
                            {"limit", limit}};
  const QString key = buildRequestKey(request);
  currentBuildItemRequest_ = key;
  buildSourceItemsError_.clear();
  if (const auto cached = buildSourceCache_.constFind(key);
      cached != buildSourceCache_.cend()) {
    QString parseError;
    if (!buildSourceItems_.replace(cached.value(), &parseError)) {
      buildSourceItemsError_ = parseError;
    }
  } else {
    buildSourceItems_.clear();
    if (!pendingBuildRequests_.contains(key)) {
      pendingBuildRequests_.insert(key);
      daemon_.searchBuildItems(query, category, limit);
    }
  }
  emit buildSourceItemsStateChanged();
}

void AppController::requestSourceBuilds(const QString &item,
                                        const QString &query,
                                        const QString &scope,
                                        const QString &ordering, int limit,
                                        int offset, bool refresh) {
  const QJsonObject request{{"op", "build_list"},
                            {"item", item},
                            {"query", query},
                            {"scope", scope},
                            {"ordering", ordering},
                            {"limit", limit},
                            {"offset", offset},
                            {"refresh", refresh}};
  const QString key = buildRequestKey(request);
  currentBuildListRequest_ = key;
  sourceBuildsError_.clear();
  if (!refresh) {
    if (const auto cached = buildSourceCache_.constFind(key);
        cached != buildSourceCache_.cend()) {
      QString parseError;
      if (!sourceBuilds_.replace(cached.value(), &parseError)) {
        sourceBuildsError_ = parseError;
      }
      emit sourceBuildsStateChanged();
      return;
    }
  }
  sourceBuilds_.clear();
  if (!pendingBuildRequests_.contains(key)) {
    pendingBuildRequests_.insert(key);
    daemon_.requestBuildList(item, query, scope, ordering, limit, offset,
                             refresh);
  }
  emit sourceBuildsStateChanged();
}

void AppController::requestBuildRevision(qint64 id, bool refresh) {
  if (id <= 0) {
    return;
  }
  const QJsonObject request{{"op", "build_detail"},
                            {"build_id", id},
                            {"refresh", refresh}};
  const QString key = buildRequestKey(request);
  buildRevisionErrors_.remove(id);
  if (!refresh) {
    if (const auto cached = buildSourceCache_.constFind(key);
        cached != buildSourceCache_.cend()) {
      buildRevisions_.insert(id, cached.value());
      emit buildRevisionChanged(id);
      return;
    }
  }
  if (!pendingBuildRequests_.contains(key)) {
    pendingBuildRequests_.insert(key);
    daemon_.requestBuildDetail(id, refresh);
  }
  emit buildRevisionChanged(id);
}

void AppController::refreshActivity() { daemon_.requestActivity(); }

void AppController::setFissureNotificationMode(const QString &mode) {
  if (mode != "off" && mode != "session" && mode != "persistent") {
    return;
  }
  if (notificationSettingsLoaded_ && fissureNotificationMode_ == mode) {
    return;
  }
  fissureNotificationMode_ = mode;
  notificationSettingsLoaded_ = true;
  emit notificationSettingsChanged();
  daemon_.setFissureNotificationMode(mode);
}

void AppController::resolveAssets(const QJsonArray &assets) {
  const qint64 now = QDateTime::currentMSecsSinceEpoch();
  QJsonArray pending;
  for (const QJsonValue &value : assets) {
    const QJsonObject spec = value.toObject();
    const QString id = spec.value("id").toString();
    if (id.isEmpty()) {
      continue;
    }
    const bool missing = !assets_.contains(id);
    const QString identity = assetRequestIdentity(spec);
    const bool changed = assetRequestedIdentity_.value(id) != identity;
    if (missing || changed ||
        now - assetRequestedAt_.value(id, 0) >= AssetRefreshIntervalMs) {
      pending.append(value);
      assetRequestedAt_.insert(id, now);
      assetRequestedIdentity_.insert(id, identity);
    }
  }
  daemon_.requestAssets(pending);
}

void AppController::applyAssets(const QJsonArray &assets) {
  wfgui::AssetMap changedAssets;
  for (const QJsonValue &value : assets) {
    const wfgui::AssetRef asset = wfgui::AssetRef::fromJson(value.toObject());
    if (asset.isValid() && assets_.value(asset.id) != asset) {
      wfgui::acceptThumbnailAsset(asset);
      assets_.insert(asset.id, asset);
      changedAssets.insert(asset.id, asset);
    }
  }
  if (changedAssets.isEmpty()) {
    return;
  }
  relics_.applyAssets(changedAssets);
  foundryItems_.applyAssets(changedAssets);
  inventoryItems_.applyAssets(changedAssets);
  masteryItems_.applyAssets(changedAssets);
  buildEquipment_.applyAssets(changedAssets);
  QStringList ids = changedAssets.keys();
  ids.sort();
  emit assetsChanged(ids);
}

void AppController::applyAssetRefresh(const QJsonObject &asset) {
  const QString identity = assetRequestIdentity(asset);
  QJsonArray refreshed;
  for (auto requested = assetRequestedIdentity_.cbegin();
       requested != assetRequestedIdentity_.cend(); ++requested) {
    if (requested.value() != identity) {
      continue;
    }
    QJsonObject resolved = asset;
    resolved.insert("id", requested.key());
    refreshed.append(resolved);
  }
  applyAssets(refreshed);
}

void AppController::resolveMarketQuotes(const QStringList &items,
                                        bool refresh) {
  constexpr qint64 RetryAfterMs = 15 * 60'000;
  const qint64 now = QDateTime::currentMSecsSinceEpoch();
  QStringList pending;
  for (const QString &item : items) {
    if (item.isEmpty()) {
      continue;
    }
    const qint64 requestedAt = marketRequestedAt_.value(item, 0);
    if (refresh || requestedAt == 0 || now - requestedAt >= RetryAfterMs) {
      marketRequestedAt_.insert(item, now);
      pending.append(item);
    }
  }
  daemon_.requestMarketQuotes(pending, refresh);
}

void AppController::requestMarketVariantQuote(const QString &item,
                                              const QJsonObject &filters,
                                              bool refresh) {
  const QString key = marketVariantKey(item, filters);
  const QJsonObject cached = marketVariantQuote(item, filters);
  const qint64 now = QDateTime::currentMSecsSinceEpoch();
  const qint64 requestedAt = marketVariantRequestedAt_.value(key, 0);
  const bool fresh = requestedAt > 0 && now - requestedAt < 60'000;
  if (!refresh && !cached.isEmpty() && fresh) {
    QTimer::singleShot(0, this, [this, item, filters] {
      emit marketVariantQuoteReady(item, filters, marketVariantQuote(item, filters));
    });
    return;
  }
  if (marketVariantPending_.contains(key) ||
      (!refresh && cached.isEmpty() && fresh)) {
    return;
  }
  marketVariantPending_.insert(key);
  marketVariantRequestedAt_.insert(key, now);
  daemon_.requestMarketVariantQuote(item, filters,
                                    refresh || !cached.isEmpty());
}

void AppController::searchMarketItems(const QString &query, int limit) {
  daemon_.requestMarketMatches(query, limit);
}

void AppController::describeMarketItems(const QStringList &items) {
  QStringList missing;
  for (const QString &item : items) {
    if (!item.isEmpty() && !marketItems_.contains(marketKey(item))) {
      missing.append(item);
    }
  }
  daemon_.requestMarketItems(missing);
}

void AppController::ensureMarket() {
  if (!marketLoaded_ && !marketPending_) {
    refreshMarket();
  }
}

void AppController::refreshMarket() {
  if (marketPending_) {
    return;
  }
  marketPending_ = true;
  marketError_.clear();
  emit marketAccountChanged();
  daemon_.requestMarketAccount();
}

void AppController::marketLogin(const QString &email, const QString &password) {
  beginMarketAction();
  daemon_.marketLogin(email, password);
}

void AppController::marketLogout() {
  beginMarketAction();
  daemon_.marketLogout();
}

void AppController::marketCreateOrder(const QJsonObject &order) {
  beginMarketAction();
  daemon_.marketCreateOrder(order);
}

void AppController::marketUpdateOrder(const QString &id,
                                      const QJsonObject &patch) {
  beginMarketAction();
  daemon_.marketUpdateOrder(id, patch);
}

void AppController::marketDeleteOrder(const QString &id) {
  beginMarketAction();
  daemon_.marketDeleteOrder(id);
}

void AppController::marketCloseOrder(const QString &id, int quantity) {
  beginMarketAction();
  daemon_.marketCloseOrder(id, quantity);
}

void AppController::setMarketOrdersVisible(bool visible, const QString &type) {
  beginMarketAction();
  daemon_.setMarketOrdersVisible(visible, type);
}

void AppController::setMarketPresenceMode(const QString &mode) {
  beginMarketAction();
  daemon_.setMarketPresenceMode(mode);
}

void AppController::refreshOverframeAccount() {
  if (overframeAccountPending_) {
    return;
  }
  overframeAccountPending_ = true;
  overframeAccountError_.clear();
  emit overframeAccountChanged();
  daemon_.requestOverframeAccount();
}

void AppController::importOverframeSession(const QJsonArray &cookies) {
  overframeAccountPending_ = true;
  overframeAccountError_.clear();
  emit overframeAccountChanged();
  daemon_.importOverframeSession(cookies);
}

void AppController::overframeLogout() {
  overframeAccountPending_ = true;
  overframeAccountError_.clear();
  emit overframeAccountChanged();
  daemon_.overframeLogout();
}

void AppController::refreshSourceAssetCache() {
  if (sourceAssetCacheBusy_) {
    return;
  }
  sourceAssetCacheBusy_ = true;
  sourceAssetCacheError_.clear();
  emit sourceAssetCacheChanged();
  daemon_.requestAssetCacheStatus();
}

void AppController::clearSourceAssetCache() {
  if (sourceAssetCacheBusy_) {
    return;
  }
  sourceAssetCacheBusy_ = true;
  sourceAssetCacheError_.clear();
  emit sourceAssetCacheChanged();
  daemon_.clearAssetCache();
}

void AppController::setError(const QString &error) {
  if (error_ == error) {
    return;
  }
  error_ = error;
  emit errorChanged();
}

void AppController::setLoading(bool loading) {
  if (loading_ == loading) {
    return;
  }
  loading_ = loading;
  emit loadingChanged();
}

void AppController::applySelectedEra() {
  const EraState &state = relicState_;
  const QJsonObject data = state.hasPrices ? state.priced : state.metadata;
  if (!data.isEmpty()) {
    QString parseError;
    if (!relics_.replace(data, &parseError)) {
      setError(parseError);
    } else {
      setError(state.error);
      emit traceCountChanged();
    }
  } else {
    relics_.clear();
    setError(state.error);
    emit traceCountChanged();
  }
  relics_.setPricesLoading(state.pricesPending);
  relics_.setAssets(assets_);
  setLoading(!state.hasMetadata && state.metadataPending);
  emit pricingChanged();
}

void AppController::requestAssets(const QJsonObject &data) {
  QJsonArray eraAssets;
  QJsonArray relicAssets;
  QJsonArray rewardAssets;
  QSet<QString> eras;
  for (const QJsonValue &itemValue : data.value("items").toArray()) {
    const QJsonObject item = itemValue.toObject();
    const QJsonObject relicAsset = item.value("asset").toObject();
    const QString relicId = relicAsset.value("id").toString();
    if (!relicId.isEmpty()) {
      relicAssets.append(relicAsset);
      const QString era = item.value("era").toString();
      if (!era.isEmpty() && !eras.contains(era)) {
        eras.insert(era);
        eraAssets.append(relicAsset);
      }
    }
    for (const QJsonValue &rewardValue : item.value("rewards").toArray()) {
      const QJsonObject rewardAsset =
          rewardValue.toObject().value("asset").toObject();
      const QString rewardId = rewardAsset.value("id").toString();
      if (!rewardId.isEmpty()) {
        rewardAssets.append(rewardAsset);
      }
    }
    for (const QJsonValue &componentValue :
         item.value("components").toArray()) {
      const QJsonObject componentAsset =
          componentValue.toObject().value("asset").toObject();
      const QString componentId = componentAsset.value("id").toString();
      if (!componentId.isEmpty()) {
        rewardAssets.append(componentAsset);
      }
    }
  }
  resolveAssets(eraAssets);
  resolveAssets(relicAssets);
  resolveAssets(rewardAssets);
}

void AppController::handlePlayerDatasetChanged(qint64 revision,
                                               const QString &source) {
  if (revision < 0 || revision == playerRevision_ ||
      (!source.isEmpty() && source != "inventory" && source != "clear")) {
    return;
  }
  playerRevision_ = revision;
  foundryState_.stale = true;
  inventoryState_.stale = true;
  masteryState_.stale = true;
  buildEquipmentState_.stale = true;
  buildGroupsLoaded_ = false;
  relicState_.stale = true;

  if (activePage_ == "build-planner") {
    refreshBuildGroups();
  }

  if (activePage_ == "foundry" && !foundryState_.pending) {
    refreshFoundry();
  } else if (activePage_ == "inventory" && !inventoryState_.pending) {
    refreshInventory();
  } else if (activePage_ == "mastery" && !masteryState_.pending) {
    refreshMastery();
  } else if (activePage_ == "build-planner" &&
             !buildEquipmentState_.pending) {
    refreshBuildEquipment();
  } else if (activePage_ == "relic" && !relicState_.metadataPending &&
             !relicState_.pricesPending) {
    refresh();
  } else if (activePage_ == "market" && !inventoryState_.pending) {
    refreshInventory();
  }
}

void AppController::applyPlayerView(const QString &view,
                                    const QJsonObject &data) {
  if (view == "build_equipment") {
    const qint64 revision = data.value("player_revision").toInteger(-1);
    buildEquipmentState_.pending = false;
    if (playerRevision_ >= 0 && revision >= 0 && revision < playerRevision_) {
      buildEquipmentState_.stale = true;
      if (activePage_ == "build-planner") {
        refreshBuildEquipment();
      }
      return;
    }
    QString parseError;
    if (!buildEquipment_.replace(data, &parseError)) {
      buildEquipmentState_.error = parseError;
    } else {
      buildEquipmentState_.loaded = true;
      buildEquipmentState_.stale = false;
      buildEquipmentState_.revision = revision;
      buildEquipmentState_.error.clear();
      buildEquipment_.setAssets(assets_);
      resolveAssets(buildAssets(data));
    }
    emit buildEquipmentStateChanged();
    return;
  }
  PlayerViewState *state = playerState(view);
  PlayerItemModel *model = playerModel(view);
  if (!state || !model) {
    return;
  }
  const qint64 revision = data.value("revision").toInteger(-1);
  if (playerRevision_ >= 0 && revision >= 0 && revision < playerRevision_) {
    state->pending = false;
    state->stale = true;
    if (activePage_ == view ||
        (activePage_ == "market" && view == "inventory")) {
      if (view == "foundry") {
        refreshFoundry();
      } else if (view == "inventory") {
        refreshInventory();
      } else if (view == "mastery") {
        refreshMastery();
      }
    }
    return;
  }
  QString parseError;
  state->pending = false;
  if (!model->replace(data, &parseError)) {
    state->error = parseError;
  } else {
    state->loaded = true;
    state->stale = false;
    state->revision = revision;
    state->error.clear();
    state->summary = data.value("summary").toObject();
    const QJsonObject profile = data.value("profile").toObject();
    if (!profile.isEmpty() && profile != playerProfile_) {
      playerProfile_ = profile;
      emit playerProfileChanged();
    }
    const QJsonObject rankAsset = profile.value("rank_asset").toObject();
    if (!rankAsset.value("id").toString().isEmpty()) {
      resolveAssets(QJsonArray{rankAsset});
    }
    model->setAssets(assets_);
  }
  emitPlayerStateChanged(view);
}

AppController::PlayerViewState *
AppController::playerState(const QString &view) {
  if (view == "foundry") {
    return &foundryState_;
  }
  if (view == "inventory") {
    return &inventoryState_;
  }
  if (view == "mastery") {
    return &masteryState_;
  }
  if (view == "build_equipment") {
    return &buildEquipmentState_;
  }
  return nullptr;
}

PlayerItemModel *AppController::playerModel(const QString &view) {
  if (view == "foundry") {
    return &foundryItems_;
  }
  if (view == "inventory") {
    return &inventoryItems_;
  }
  if (view == "mastery") {
    return &masteryItems_;
  }
  return nullptr;
}

void AppController::emitPlayerStateChanged(const QString &view) {
  if (view == "foundry") {
    emit foundryStateChanged();
  } else if (view == "inventory") {
    emit inventoryStateChanged();
  } else if (view == "mastery") {
    emit masteryStateChanged();
  } else if (view == "build_equipment") {
    emit buildEquipmentStateChanged();
  }
}

void AppController::applyMarketDescriptors(const QJsonArray &items) {
  QJsonArray assets;
  bool changed = false;
  for (const QJsonValue &value : items) {
    const QJsonObject item = value.toObject();
    if (item.isEmpty()) {
      continue;
    }
    const QStringList keys = {item.value("id").toString(),
                              item.value("slug").toString(),
                              item.value("name").toString()};
    for (const QString &key : keys) {
      if (!key.isEmpty() && marketItems_.value(marketKey(key)) != item) {
        marketItems_.insert(marketKey(key), item);
        changed = true;
      }
    }
    const QJsonObject asset = item.value("asset").toObject();
    if (!asset.isEmpty()) {
      assets.append(asset);
    }
  }
  resolveAssets(assets);
  if (changed) {
    emit marketCatalogChanged();
  }
}

void AppController::beginMarketAction() {
  ++marketActions_;
  marketError_.clear();
  emit marketAccountChanged();
}

void AppController::finishMarketAction() {
  marketActions_ = qMax(0, marketActions_ - 1);
}

QString AppController::marketKey(const QString &value) {
  return value.trimmed().toCaseFolded();
}

QString AppController::marketVariantKey(const QString &item,
                                        const QJsonObject &filters) {
  return marketKey(item) + ':' +
         QString::fromUtf8(
             QJsonDocument(filters).toJson(QJsonDocument::Compact));
}

QString AppController::buildRequestKey(const QJsonObject &request) {
  QJsonObject identity = request;
  identity.remove("refresh");
  return QString::fromUtf8(
      QJsonDocument(identity).toJson(QJsonDocument::Compact));
}

void AppController::applyBuildSourceReply(const QJsonObject &request,
                                          const QJsonObject &data) {
  const QString key = buildRequestKey(request);
  pendingBuildRequests_.remove(key);

  const QString op = request.value("op").toString();
  if (op == "build_group_plan") {
    pendingBuildGroupRequests_.remove(key);
    buildGroupsError_.clear();
    const QString id = request.value("group_id").toString();
    QJsonObject group = buildGroups_.group(id);
    if (group.value("revision").toInteger(-1) == data.value("group_revision").toInteger(-2) &&
        group.value("baseline").toObject().value("fingerprint") == data.value("target_fingerprint")) {
      group.insert("plan_result", data);
      buildGroups_.upsert(group);
    }
    emit buildGroupsStateChanged();
    emit buildGroupRequestFinished(request, data);
    requestBuildGroup(id);
    return;
  }
  if (op.startsWith("build_group_")) {
    pendingBuildGroupRequests_.remove(key);
    buildGroupsError_.clear();
    const qint64 revision = data.value("player_revision").toInteger(-1);
    if (revision >= 0 && revision < playerRevision_) {
      buildGroupsLoaded_ = false;
      emit buildGroupRequestFinished(request, data);
      refreshBuildGroups();
      return;
    }
    if (op == "build_group_list") {
      QString parseError;
      if (!buildGroups_.replace(data, &parseError)) {
        buildGroupsError_ = parseError;
      } else {
        buildGroupsLoaded_ = true;
      }
    } else if (op == "build_group_delete") {
      buildGroups_.remove(data.value("id").toString());
    } else {
      QString parseError;
      if (!buildGroups_.upsert(data, &parseError)) {
        buildGroupsError_ = parseError;
      }
    }
    emit buildGroupsStateChanged();
    emit buildGroupRequestFinished(request, data);
    return;
  }

  buildSourceCache_.insert(key, data);
  if (op == "build_search") {
    if (key != currentBuildItemRequest_) {
      return;
    }
    QString parseError;
    buildSourceItemsError_.clear();
    if (!buildSourceItems_.replace(data, &parseError)) {
      buildSourceItemsError_ = parseError;
    }
    emit buildSourceItemsStateChanged();
    return;
  }
  if (op == "build_list") {
    if (key != currentBuildListRequest_) {
      return;
    }
    QString parseError;
    sourceBuildsError_.clear();
    if (!sourceBuilds_.replace(data, &parseError)) {
      sourceBuildsError_ = parseError;
    }
    emit sourceBuildsStateChanged();
    return;
  }
  if (op != "build_detail") {
    return;
  }

  const qint64 id = request.value("build_id").toInteger();
  const QJsonObject identity = data.value("identity").toObject();
  if (identity.value("external_id").toInteger() != id ||
      !data.value("content").isObject() ||
      data.value("fingerprint").toString().isEmpty()) {
    buildRevisionErrors_.insert(id, "daemon returned malformed build revision");
  } else {
    buildRevisionErrors_.remove(id);
    buildRevisions_.insert(id, data);
  }
  emit buildRevisionChanged(id);
}

void AppController::applyBuildSourceError(const QJsonObject &request,
                                          const QString &error) {
  const QString key = buildRequestKey(request);
  pendingBuildRequests_.remove(key);
  const QString op = request.value("op").toString();
  if (op.startsWith("build_group_")) {
    pendingBuildGroupRequests_.remove(key);
    buildGroupsError_ = error;
    emit buildGroupsStateChanged();
    emit buildGroupRequestFailed(request, error);
    return;
  }
  if (op == "build_search" && key == currentBuildItemRequest_) {
    buildSourceItemsError_ = error;
    emit buildSourceItemsStateChanged();
  } else if (op == "build_list" && key == currentBuildListRequest_) {
    sourceBuildsError_ = error;
    emit sourceBuildsStateChanged();
  } else if (op == "build_detail") {
    const qint64 id = request.value("build_id").toInteger();
    buildRevisionErrors_.insert(id, error);
    emit buildRevisionChanged(id);
  }
}
