#include "daemon_client.h"
#include "wfgui_paths.h"

#include <QCoreApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLocalSocket>
#include <QProcessEnvironment>
#include <QStandardPaths>
#include <QStringList>
#include <QTimer>

#include <algorithm>
#include <ranges>
#include <utility>

namespace {
constexpr qint64 HelloRequestId = 1;
constexpr qint64 PlayerSubscriptionId = 2;
constexpr int AssetBatchSize = 8;
constexpr int MaxActiveAssetRequests = 2;
constexpr int MarketCacheBatchSize = 100;
constexpr int MarketDescriptionBatchSize = 100;
constexpr int MaxActiveMarketDescriptionRequests = 2;
constexpr int MaxActiveMarketQuoteRequests = 3;
constexpr int MaxActiveBuildRequests = 4;
constexpr int MarketQuoteTtlSeconds = 15 * 60;
constexpr qsizetype MaximumFrameBytes = 64 * 1024 * 1024;
constexpr qsizetype ReadChunkBytes = 64 * 1024;
constexpr int DispatchBatchSize = 8;
constexpr int DispatchBudgetMs = 4;

int replyTimeout(const QString &operation) {
  if (operation == "hello") {
    return 10'000;
  }
  if (operation == "build_group_plan") {
    return 10 * 60'000;
  }
  if (operation.startsWith("asset_cache_")) {
    return 5 * 60'000;
  }
  return 120'000;
}

QJsonObject guiInterfaces() {
  return {
      {"datasets", WFCLI_INTERFACE_DATASETS},
      {"player", WFCLI_INTERFACE_PLAYER},
      {"worldstate", WFCLI_INTERFACE_WORLDSTATE},
      {"notifications", WFCLI_INTERFACE_NOTIFICATIONS},
      {"market", WFCLI_INTERFACE_MARKET},
      {"overframe", WFCLI_INTERFACE_OVERFRAME},
      {"relics", WFCLI_INTERFACE_RELICS},
      {"assets", WFCLI_INTERFACE_ASSETS},
      {"builds", WFCLI_INTERFACE_BUILDS},
      {"diagnostics", WFCLI_INTERFACE_DIAGNOSTICS},
  };
}

bool validProtocolNumber(const QJsonValue &value) {
  return value.isDouble() && value.toInteger() > 0 &&
         value.toDouble() == static_cast<double>(value.toInteger());
}

bool isBuildMutation(const QJsonObject &request) {
  const QString op = request.value("op").toString();
  return op == "build_group_create" || op == "build_group_update" ||
         op == "build_group_delete" || op == "build_group_add_source" ||
         op == "build_group_add_config" || op == "build_group_remove_member" ||
         op == "build_group_plan";
}
} // namespace

bool wfgui::daemonContractNeedsUpdate(const QJsonObject &reply,
                                      const QJsonObject &requiredInterfaces,
                                      int requiredEnvelope) {
  if (!reply.contains("envelope") &&
      validProtocolNumber(reply.value("protocol"))) {
    return true;
  }
  const QJsonValue offeredEnvelope = reply.value("envelope");
  const QJsonValue offeredInterfaces = reply.value("interfaces");
  if (!validProtocolNumber(offeredEnvelope) || !offeredInterfaces.isObject()) {
    return false;
  }

  bool mismatch = false;
  const qint64 envelope = offeredEnvelope.toInteger();
  if (envelope != requiredEnvelope) {
    mismatch = true;
    if (requiredEnvelope <= envelope) {
      return false;
    }
  }

  const QJsonObject offered = offeredInterfaces.toObject();
  for (auto required = requiredInterfaces.constBegin();
       required != requiredInterfaces.constEnd(); ++required) {
    const QJsonValue available = offered.value(required.key());
    if (available == required.value()) {
      continue;
    }
    mismatch = true;
    if (!validProtocolNumber(required.value())) {
      return false;
    }
    if (!available.isUndefined() && !available.isNull() &&
        (!validProtocolNumber(available) ||
         required.value().toInteger() <= available.toInteger())) {
      return false;
    }
  }
  return mismatch;
}

DaemonClient::DaemonClient(QObject *parent)
    : QObject(parent), socket_(new QLocalSocket(this)),
      reconnectTimer_(new QTimer(this)), inputTimer_(new QTimer(this)),
      replyTimer_(new QTimer(this)) {
  reconnectTimer_->setInterval(1000);
  reconnectTimer_->setSingleShot(true);
  socket_->setReadBufferSize(4 * ReadChunkBytes);
  inputTimer_->setSingleShot(true);
  replyTimer_->setObjectName("replyDeadline");
  replyTimer_->setSingleShot(true);
  replyTimer_->setTimerType(Qt::PreciseTimer);
  connect(inputTimer_, &QTimer::timeout, this, &DaemonClient::drainInput);
  connect(replyTimer_, &QTimer::timeout, this, [this] {
    if (replies_.isEmpty()) {
      return;
    }
    const auto oldest =
        std::min_element(replies_.cbegin(), replies_.cend(),
                         [](const PendingReply &a, const PendingReply &b) {
                           return a.deadline < b.deadline;
                         });
    const QString operation = oldest->operation;
    socket_->abort();
    setStatus(QString("Timed out waiting for Retinue daemon (%1)").arg(operation));
  });

  connect(reconnectTimer_, &QTimer::timeout, this,
          &DaemonClient::connectSocket);
  connect(socket_, &QLocalSocket::connected, this, [this] {
    setStatus("Negotiating with Retinue daemon");
    sendHello();
  });
  connect(socket_, &QLocalSocket::readyRead, this, [this] {
    if (!inputTimer_->isActive()) {
      drainInput();
    }
  });
  connect(socket_, &QLocalSocket::disconnected, this, [this] {
    ready_ = false;
    setConnected(false);
    input_.clear();
    scanOffset_ = 0;
    inputTimer_->stop();
    replyTimer_->stop();
    replies_.clear();
    for (const RelicRequest &request : std::as_const(activeRelicRequests_)) {
      pendingRelicRequests_.insert(relicRequestKey(request), request);
    }
    for (const QJsonArray &assets : std::as_const(activeAssetRequests_)) {
      for (const QJsonValue &asset : assets) {
        const QJsonObject spec = asset.toObject();
        const QString id = spec.value("id").toString();
        if (!id.isEmpty()) {
          pendingAssets_.insert(id, spec);
          pendingAssetOrder_.removeAll(id);
          pendingAssetOrder_.append(id);
        }
      }
    }
    for (const QString &view : std::as_const(activePlayerViews_)) {
      pendingPlayerViews_.insert(view);
    }
    pendingActivity_ = pendingActivity_ || activeActivityRequest_ != 0;
    if (activeAssetCacheRequest_ != 0) {
      if (activeAssetCacheClear_) {
        emit assetCacheRequestFailed(
            "Retinue daemon disconnected; cache clear result is unknown");
      }
      pendingAssetCacheStatus_ = true;
    }
    pendingNotificationSettings_ = true;
    for (const MarketQuoteRequest &request :
         std::as_const(activeMarketQuoteRequests_)) {
      for (const QString &item : request.items) {
        if (request.cacheOnly) {
          marketCacheRequested_.remove(item);
          pendingMarketCacheQuotes_.insert(item);
          pendingMarketCacheQuoteOrder_.removeAll(item);
          pendingMarketCacheQuoteOrder_.append(item);
        } else {
          pendingMarketQuotes_.insert(item, pendingMarketQuotes_.value(item) ||
                                                request.refresh);
          pendingMarketQuoteOrder_.removeAll(item);
          pendingMarketQuoteOrder_.append(item);
        }
      }
    }
    for (const MarketVariantRequest &request :
         std::as_const(activeMarketVariantRequests_)) {
      pendingMarketVariantRequests_.prepend(request);
    }
    if (!pendingMarketResolve_.has_value() &&
        !activeMarketResolveRequests_.isEmpty()) {
      pendingMarketResolve_ = activeMarketResolveRequests_.constBegin().value();
    }
    for (const QStringList &items :
         std::as_const(activeMarketDescriptionRequests_)) {
      for (const QString &item : items) {
        pendingMarketDescriptions_.insert(item);
        pendingMarketDescriptionOrder_.removeAll(item);
        pendingMarketDescriptionOrder_.append(item);
      }
    }
    for (const QString &action : std::as_const(activeMarketAccountRequests_)) {
      if (action == "snapshot") {
        pendingMarketAccountSnapshot_ = true;
      } else if (action == "presence") {
        emit marketPresenceFailed(
            "Retinue daemon disconnected; presence update result is unknown");
      } else {
        emit marketAccountFailed(
            action, "Retinue daemon disconnected; operation result is unknown");
        pendingMarketAccountSnapshot_ = true;
      }
    }
    for (const QString &action :
         std::as_const(activeOverframeAccountRequests_)) {
      if (action == "snapshot") {
        pendingOverframeAccountSnapshot_ = true;
      } else {
        emit overframeAccountFailed(
            action, "Retinue daemon disconnected; operation result is unknown");
        pendingOverframeAccountSnapshot_ = true;
      }
    }
    bool reconcileBuildGroups = false;
    for (const QJsonObject &request : std::as_const(activeBuildRequests_)) {
      if (isBuildMutation(request)) {
        emit buildSourceFailed(
            request, "Retinue daemon disconnected; operation result is unknown");
        reconcileBuildGroups = true;
      } else if (!pendingBuildRequests_.contains(request)) {
        pendingBuildRequests_.prepend(request);
      }
    }
    if (reconcileBuildGroups) {
      const QJsonObject list{{"op", "build_group_list"}};
      if (!pendingBuildRequests_.contains(list)) {
        pendingBuildRequests_.append(list);
      }
    }
    activeRelicRequests_.clear();
    activePlayerViews_.clear();
    activeActivityRequest_ = 0;
    activeNotificationSettingsRequest_ = 0;
    activeNotificationRequestIsSet_ = false;
    sentNotificationMode_.reset();
    activeAssetRequests_.clear();
    activeAssetCacheRequest_ = 0;
    activeAssetCacheClear_ = false;
    activeMarketQuoteRequests_.clear();
    activeMarketVariantRequests_.clear();
    activeMarketResolveRequests_.clear();
    activeMarketDescriptionRequests_.clear();
    activeMarketAccountRequests_.clear();
    activeOverframeAccountRequests_.clear();
    activeBuildRequests_.clear();
    activeDiagnosticsRequest_ = 0;
    diagnosticsDirty_ = true;
    negotiatedFeatures_.clear();
    ready_ = false;
    setConnected(false);
    setStatus("Retinue daemon disconnected");
    if (!reconnectTimer_->isActive()) {
      reconnectTimer_->start();
    }
  });
  connect(socket_, &QLocalSocket::errorOccurred, this,
          [this](QLocalSocket::LocalSocketError error) {
            if (error == QLocalSocket::ServerNotFoundError ||
                error == QLocalSocket::ConnectionRefusedError) {
              ensureDaemon();
            } else {
              setStatus(socket_->errorString());
            }
            if (!reconnectTimer_->isActive()) {
              reconnectTimer_->start();
            }
          });

  connect(&ensureProcess_,
          qOverload<int, QProcess::ExitStatus>(&QProcess::finished), this,
          [this](int exitCode, QProcess::ExitStatus exitStatus) {
            const bool succeeded =
                exitStatus == QProcess::NormalExit && exitCode == 0;
            if (!succeeded) {
              if (stoppingDaemon_) {
                setStatus("Could not restart Retinue daemon");
              } else if (updatingDaemon_) {
                setStatus("Could not update Retinue daemon");
              } else {
                setStatus("Could not start Retinue daemon");
              }
            }
            if (stoppingDaemon_ && succeeded) {
              stoppingDaemon_ = false;
              ensureAttempted_ = false;
              ensureDaemon();
              return;
            }
            stoppingDaemon_ = false;
            if (!reconnectTimer_->isActive()) {
              reconnectTimer_->start(250);
            }
          });
}

DaemonClient::~DaemonClient() {
  reconnectTimer_->stop();
  socket_->abort();
  if (ensureProcess_.state() == QProcess::NotRunning) {
    return;
  }
  ensureProcess_.terminate();
  if (!ensureProcess_.waitForFinished(500)) {
    ensureProcess_.kill();
    ensureProcess_.waitForFinished(500);
  }
}

bool DaemonClient::connected() const { return connected_; }

QString DaemonClient::status() const { return status_; }

bool DaemonClient::marketQuoteFetchBusy() const {
  return marketQuotePhaseBusy(false);
}

void DaemonClient::start() { connectSocket(); }

void DaemonClient::requestRelics(const QString &era, bool fetchPrices) {
  const RelicRequest request{.era = era, .prices = fetchPrices};
  const QString key = relicRequestKey(request);
  pendingRelicRequests_.insert(key, request);
  sendPendingRequests();
}

void DaemonClient::requestPlayerView(const QString &view) {
  pendingPlayerViews_.insert(view);
  sendPendingPlayerViews();
}

void DaemonClient::requestActivity() {
  if (activeActivityRequest_ == 0) {
    pendingActivity_ = true;
  }
  sendPendingActivity();
}

void DaemonClient::requestNotificationSettings() {
  pendingNotificationSettings_ = true;
  sendPendingNotificationSettings();
}

void DaemonClient::setFissureNotificationMode(const QString &mode) {
  if (mode != "off" && mode != "session" && mode != "persistent") {
    return;
  }
  desiredNotificationMode_ = mode;
  sendPendingNotificationSettings();
}

void DaemonClient::requestAssets(const QJsonArray &assets) {
  QStringList requested;
  for (const QJsonValue &asset : assets) {
    const QJsonObject spec = asset.toObject();
    const QString id = spec.value("id").toString();
    if (!id.isEmpty() && !assetRequestActive(id)) {
      pendingAssets_.insert(id, spec);
      pendingAssetOrder_.removeAll(id);
      requested.append(id);
    }
  }
  for (auto id = requested.crbegin(); id != requested.crend(); ++id) {
    pendingAssetOrder_.prepend(*id);
  }
  sendPendingAssets();
}

void DaemonClient::requestAssetCacheStatus() {
  pendingAssetCacheStatus_ = true;
  sendPendingAssetCacheRequest();
}

void DaemonClient::clearAssetCache() {
  pendingAssetCacheClear_ = true;
  sendPendingAssetCacheRequest();
}

void DaemonClient::requestMarketQuotes(const QStringList &items, bool refresh) {
  QStringList cacheRequested;
  QStringList requested;
  for (const QString &item : items) {
    if (item.isEmpty()) {
      continue;
    }
    if (!refresh && !marketCacheRequested_.contains(item)) {
      marketCacheRequested_.insert(item);
      pendingMarketCacheQuotes_.insert(item);
      pendingMarketCacheQuoteOrder_.removeAll(item);
      cacheRequested.append(item);
    }
    if (marketQuoteActive(item, false) && !refresh) {
      continue;
    }
    pendingMarketQuotes_.insert(item,
                                pendingMarketQuotes_.value(item) || refresh);
    pendingMarketQuoteOrder_.removeAll(item);
    requested.append(item);
  }
  for (auto item = requested.crbegin(); item != requested.crend(); ++item) {
    pendingMarketQuoteOrder_.prepend(*item);
  }
  for (auto item = cacheRequested.crbegin(); item != cacheRequested.crend();
       ++item) {
    pendingMarketCacheQuoteOrder_.prepend(*item);
  }
  sendPendingMarketQuotes();
}

void DaemonClient::requestMarketVariantQuote(const QString &item,
                                             const QJsonObject &filters,
                                             bool refresh) {
  if (item.isEmpty() || filters.isEmpty()) {
    return;
  }
  pendingMarketVariantRequests_.append(
      {.item = item, .filters = filters, .refresh = refresh});
  sendPendingMarketVariantQuotes();
}

void DaemonClient::requestMarketMatches(const QString &query, int limit) {
  const QString normalized = query.trimmed();
  if (normalized.isEmpty()) {
    return;
  }
  pendingMarketResolve_ =
      MarketResolveRequest{.query = normalized, .limit = qBound(1, limit, 5)};
  sendPendingMarketResolve();
}

void DaemonClient::requestMarketItems(const QStringList &items) {
  for (const QString &item : items) {
    if (item.isEmpty() || pendingMarketDescriptions_.contains(item)) {
      continue;
    }
    bool active = false;
    for (const QStringList &request : activeMarketDescriptionRequests_) {
      if (request.contains(item)) {
        active = true;
        break;
      }
    }
    if (!active) {
      pendingMarketDescriptions_.insert(item);
      pendingMarketDescriptionOrder_.append(item);
    }
  }
  sendPendingMarketDescriptions();
}

void DaemonClient::requestMarketAccount() {
  pendingMarketAccountSnapshot_ = true;
  sendPendingMarketAccount();
}

void DaemonClient::marketLogin(const QString &email, const QString &password) {
  queueMarketAccountRequest(
      "login",
      {{"op", "market_login"}, {"email", email}, {"password", password}});
}

void DaemonClient::marketLogout() {
  queueMarketAccountRequest("logout", {{"op", "market_logout"}});
}

void DaemonClient::requestOverframeAccount() {
  pendingOverframeAccountSnapshot_ = true;
  sendPendingOverframeAccount();
}

void DaemonClient::importOverframeSession(const QJsonArray &cookies) {
  queueOverframeAccountRequest(
      "import", {{"op", "overframe_session_import"}, {"cookies", cookies}});
}

void DaemonClient::overframeLogout() {
  queueOverframeAccountRequest("logout", {{"op", "overframe_logout"}});
}

void DaemonClient::searchBuildItems(const QString &query,
                                    const QString &category, int limit) {
  queueBuildRequest({{"op", "build_search"},
                     {"query", query},
                     {"class", category},
                     {"limit", limit}});
}

void DaemonClient::requestBuildList(const QString &item, const QString &query,
                                    const QString &scope,
                                    const QString &ordering, int limit,
                                    int offset, bool refresh) {
  queueBuildRequest({{"op", "build_list"},
                     {"item", item},
                     {"query", query},
                     {"scope", scope},
                     {"ordering", ordering},
                     {"limit", limit},
                     {"offset", offset},
                     {"refresh", refresh}});
}

void DaemonClient::requestBuildDetail(qint64 buildId, bool refresh) {
  queueBuildRequest(
      {{"op", "build_detail"}, {"build_id", buildId}, {"refresh", refresh}});
}

void DaemonClient::requestBuildGroups() {
  queueBuildRequest({{"op", "build_group_list"}});
}

void DaemonClient::requestBuildGroup(const QString &groupId) {
  queueBuildRequest({{"op", "build_group_get"}, {"group_id", groupId}});
}

void DaemonClient::createBuildGroup(const QJsonObject &group) {
  queueBuildRequest({{"op", "build_group_create"}, {"group", group}});
}

void DaemonClient::updateBuildGroup(const QString &groupId, qint64 revision,
                                    const QJsonObject &patch) {
  queueBuildRequest({{"op", "build_group_update"},
                     {"group_id", groupId},
                     {"revision", revision},
                     {"patch", patch}});
}

void DaemonClient::deleteBuildGroup(const QString &groupId, qint64 revision) {
  queueBuildRequest({{"op", "build_group_delete"},
                     {"group_id", groupId},
                     {"revision", revision}});
}

void DaemonClient::addBuildSourceToGroup(const QString &groupId,
                                         qint64 revision, qint64 externalId,
                                         const QString &fingerprint) {
  QJsonObject request{{"op", "build_group_add_source"},
                      {"group_id", groupId},
                      {"revision", revision},
                      {"source", "overframe"},
                      {"external_id", externalId}};
  if (!fingerprint.isEmpty()) {
    request.insert("fingerprint", fingerprint);
  }
  queueBuildRequest(request);
}

void DaemonClient::addBuildConfigToGroup(const QString &groupId,
                                         qint64 revision,
                                         const QString &instanceId,
                                         int configIndex) {
  queueBuildRequest({{"op", "build_group_add_config"},
                     {"group_id", groupId},
                     {"revision", revision},
                     {"instance_id", instanceId},
                     {"config_index", configIndex}});
}

void DaemonClient::removeBuildGroupMember(const QString &groupId,
                                          qint64 revision,
                                          const QString &memberId) {
  queueBuildRequest({{"op", "build_group_remove_member"},
                     {"group_id", groupId},
                     {"revision", revision},
                     {"member_id", memberId}});
}

void DaemonClient::planBuildGroup(const QString &groupId, qint64 revision) {
  queueBuildRequest({{"op", "build_group_plan"},
                     {"group_id", groupId},
                     {"revision", revision}});
}

void DaemonClient::marketCreateOrder(const QJsonObject &order) {
  queueMarketAccountRequest("create",
                            {{"op", "market_order_create"}, {"order", order}});
}

void DaemonClient::marketUpdateOrder(const QString &id,
                                     const QJsonObject &patch) {
  queueMarketAccountRequest(
      "update",
      {{"op", "market_order_update"}, {"order_id", id}, {"patch", patch}});
}

void DaemonClient::marketDeleteOrder(const QString &id) {
  queueMarketAccountRequest("delete",
                            {{"op", "market_order_delete"}, {"order_id", id}});
}

void DaemonClient::marketCloseOrder(const QString &id, int quantity) {
  queueMarketAccountRequest(
      "close",
      {{"op", "market_order_close"}, {"order_id", id}, {"quantity", quantity}});
}

void DaemonClient::setMarketOrdersVisible(bool visible, const QString &type) {
  QJsonObject message{{"op", "market_orders_visibility"}, {"visible", visible}};
  if (type == "buy" || type == "sell") {
    message.insert("type", type);
  }
  queueMarketAccountRequest("visibility", message);
}

void DaemonClient::setMarketPresenceMode(const QString &mode) {
  queueMarketAccountRequest("presence",
                            {{"op", "market_presence_set"}, {"mode", mode}});
}

void DaemonClient::setResolutionIssue(const QString &kind,
                                      const QString &identity,
                                      const QString &reason,
                                      const QString &fallback) {
  if (kind.isEmpty() || identity.isEmpty()) {
    return;
  }
  const QString key = kind + QChar::Null + identity;
  const QJsonObject issue{{"kind", kind},
                          {"identity", identity},
                          {"reason", reason},
                          {"fallback", fallback},
                          {"class", "gui"}};
  if (resolutionIssues_.value(key) == issue) {
    return;
  }
  resolutionIssues_.insert(key, issue);
  diagnosticsDirty_ = true;
  sendPendingDiagnostics();
}

void DaemonClient::clearResolutionIssue(const QString &kind,
                                        const QString &identity) {
  if (resolutionIssues_.remove(kind + QChar::Null + identity) == 0) {
    return;
  }
  diagnosticsDirty_ = true;
  sendPendingDiagnostics();
}

void DaemonClient::connectSocket() {
  if (socket_->state() != QLocalSocket::UnconnectedState) {
    return;
  }
  input_.clear();
  setStatus("Connecting to Retinue daemon");
  socket_->connectToServer(socketPath());
}

void DaemonClient::ensureDaemon(bool update) {
  if (ensureProcess_.state() != QProcess::NotRunning) {
    return;
  }
  QString action;
  if (!update) {
    if (ensureAttempted_) {
      return;
    }
    ensureAttempted_ = true;
    action = "ensure";
  } else if (!updateAttempted_) {
    updateAttempted_ = true;
    action = "update";
  } else if (!stopAttempted_) {
    stopAttempted_ = true;
    action = "stop";
  } else {
    return;
  }
  updatingDaemon_ = action == "update";
  stoppingDaemon_ = action == "stop";
  const QString command = wfcliCommand();
  if (command.isEmpty()) {
    setStatus("retinue executable not found");
    return;
  }

  QProcessEnvironment environment = QProcessEnvironment::systemEnvironment();
  for (const QString &name :
       QStringList{"LD_PRELOAD", "LD_LIBRARY_PATH", "STEAM_RUNTIME",
                   "STEAM_RUNTIME_LIBRARY_PATH"}) {
    environment.remove(name);
  }
  ensureProcess_.setProcessEnvironment(environment);
  ensureProcess_.setProgram(command);
  ensureProcess_.setArguments({"daemon", action});
  ensureProcess_.setStandardOutputFile(QProcess::nullDevice());
  ensureProcess_.setStandardErrorFile(QProcess::nullDevice());
  ensureProcess_.start();
}

void DaemonClient::handleLine(const QByteArray &line) {
  QJsonParseError parseError;
  const QJsonDocument document = QJsonDocument::fromJson(line, &parseError);
  if (parseError.error != QJsonParseError::NoError || !document.isObject()) {
    socket_->abort();
    setStatus("Retinue daemon returned invalid JSON");
    return;
  }

  const QJsonObject message = document.object();
  if (message.value("event").toString() == "dataset" &&
      message.value("subscription").toInteger() == PlayerSubscriptionId &&
      message.value("dataset").toString() == "player" &&
      message.value("data").isObject()) {
    handlePlayerSnapshot(message.value("data").toObject(),
                         message.value("source").toString());
    return;
  }
  if (message.value("event").toString() == "market_presence" &&
      message.value("data").isObject()) {
    emit marketPresenceReady(message.value("data").toObject(), false);
    return;
  }
  if (message.value("event").toString() == "asset" &&
      message.value("data").isObject()) {
    emit assetRefreshed(message.value("data").toObject());
    return;
  }
  if (message.value("event").toString() == "build_group" &&
      message.value("data").isObject()) {
    emit buildGroupEvent(message.value("action").toString(),
                         message.value("data").toObject());
    return;
  }
  if (message.contains("event")) {
    return;
  }
  if (!validProtocolNumber(message.value("id")) ||
      !message.value("ok").isBool()) {
    socket_->abort();
    setStatus("Retinue daemon returned an invalid reply envelope");
    return;
  }
  const qint64 id = message.value("id").toInteger();
  if (!replies_.remove(id)) {
    return;
  }
  armReplyDeadline();
  if (id == HelloRequestId) {
    if (!message.value("ok").toBool() ||
        !message.value("compatible").toBool()) {
      const int daemonEnvelope = message.value("envelope").toInt(-1);
      setStatus(QString("Daemon contract mismatch: GUI envelope %1, daemon %2")
                    .arg(WFCLI_LOCAL_ENVELOPE)
                    .arg(daemonEnvelope));
      if (wfgui::daemonContractNeedsUpdate(message, guiInterfaces(),
                                           WFCLI_LOCAL_ENVELOPE)) {
        ensureDaemon(true);
      }
      socket_->abort();
      return;
    }
    const QJsonObject offered = message.value("interfaces").toObject();
    const QJsonObject required = guiInterfaces();
    const bool compatibleInterfaces =
        message.value("envelope").toInt(-1) == WFCLI_LOCAL_ENVELOPE &&
        std::ranges::all_of(required.keys(), [&](const QString &name) {
          return offered.value(name).toInt(-1) ==
                 required.value(name).toInt(-2);
        });
    if (!compatibleInterfaces) {
      setStatus("Retinue daemon lacks required GUI interfaces");
      if (wfgui::daemonContractNeedsUpdate(message, required,
                                           WFCLI_LOCAL_ENVELOPE)) {
        ensureDaemon(true);
      }
      socket_->abort();
      return;
    }
    negotiatedFeatures_.clear();
    for (const QJsonValue &feature : message.value("features").toArray()) {
      if (feature.isString()) {
        negotiatedFeatures_.insert(feature.toString());
      }
    }
    ready_ = true;
    ensureAttempted_ = false;
    updateAttempted_ = false;
    stopAttempted_ = false;
    setConnected(true);
    setStatus("Connected to Retinue daemon");
    sendPlayerSubscription();
    sendPendingRequests();
    sendPendingPlayerViews();
    sendPendingActivity();
    sendPendingNotificationSettings();
    sendPendingAssets();
    sendPendingAssetCacheRequest();
    sendPendingMarketQuotes();
    sendPendingMarketVariantQuotes();
    sendPendingMarketResolve();
    sendPendingMarketDescriptions();
    sendPendingMarketAccount();
    sendPendingOverframeAccount();
    sendPendingBuildRequests();
    sendPendingDiagnostics();
    return;
  }

  if (id == PlayerSubscriptionId) {
    if (!message.value("ok").toBool() || !message.value("data").isObject()) {
      setStatus(message.value("error").toString(
          "Retinue daemon player subscription failed"));
      return;
    }
    handlePlayerSnapshot(message.value("data").toObject(), QString());
    return;
  }

  if (id == activeDiagnosticsRequest_) {
    activeDiagnosticsRequest_ = 0;
    sendPendingDiagnostics();
    return;
  }

  const auto active = activeRelicRequests_.find(id);
  if (active != activeRelicRequests_.end()) {
    const RelicRequest request = active.value();
    activeRelicRequests_.erase(active);
    if (!message.value("ok").toBool()) {
      const QString error =
          message.value("error").toString("daemon request failed");
      emit requestFailed(request.era, request.prices, error);
      sendPendingRequests();
      return;
    }
    const QJsonValue data = message.value("data");
    if (!data.isObject()) {
      emit requestFailed(request.era, request.prices,
                         "daemon returned malformed relic planner data");
      sendPendingRequests();
      return;
    }
    emit relicPlannerReady(request.era, request.prices, data.toObject());
    sendPendingRequests();
    return;
  }

  const auto assetRequest = activeAssetRequests_.find(id);
  if (assetRequest != activeAssetRequests_.end()) {
    activeAssetRequests_.erase(assetRequest);
    if (!message.value("ok").toBool()) {
      emit assetRequestFailed(
          message.value("error").toString("asset request failed"));
      sendPendingAssets();
      return;
    }
    const QJsonArray assets =
        message.value("data").toObject().value("assets").toArray();
    emit assetsResolved(assets);
    sendPendingAssets();
    return;
  }

  if (id == activeAssetCacheRequest_) {
    activeAssetCacheRequest_ = 0;
    activeAssetCacheClear_ = false;
    if (message.value("ok").toBool() && message.value("data").isObject()) {
      emit assetCacheStatusReady(message.value("data").toObject());
    } else {
      emit assetCacheRequestFailed(
          message.value("error").toString("asset cache request failed"));
    }
    sendPendingAssetCacheRequest();
    return;
  }

  if (id == activeActivityRequest_) {
    activeActivityRequest_ = 0;
    if (!message.value("ok").toBool()) {
      emit activityFailed(
          message.value("error").toString("activity request failed"));
      return;
    }
    const QJsonValue data = message.value("data");
    if (!data.isObject()) {
      emit activityFailed("daemon returned malformed activity data");
      return;
    }
    emit activityReady(data.toObject());
    return;
  }

  if (id == activeNotificationSettingsRequest_) {
    activeNotificationSettingsRequest_ = 0;
    const bool wasSet = activeNotificationRequestIsSet_;
    activeNotificationRequestIsSet_ = false;
    const std::optional<QString> sentMode = sentNotificationMode_;
    sentNotificationMode_.reset();
    if (!message.value("ok").toBool()) {
      desiredNotificationMode_.reset();
      emit notificationSettingsFailed(message.value("error").toString(
          "notification settings request failed"));
      return;
    }
    const QJsonValue data = message.value("data");
    if (!data.isObject()) {
      desiredNotificationMode_.reset();
      emit notificationSettingsFailed(
          "daemon returned malformed notification settings");
      return;
    }
    pendingNotificationSettings_ = false;
    const bool superseded =
        wasSet && sentMode.has_value() &&
        desiredNotificationMode_.has_value() &&
        desiredNotificationMode_.value() != sentMode.value();
    if (wasSet && !superseded) {
      desiredNotificationMode_.reset();
    }
    if (!superseded) {
      emit notificationSettingsReady(data.toObject());
    }
    sendPendingNotificationSettings();
    return;
  }

  const auto marketRequest = activeMarketQuoteRequests_.find(id);
  if (marketRequest != activeMarketQuoteRequests_.end()) {
    const QStringList items = marketRequest->items;
    const bool cacheOnly = marketRequest->cacheOnly;
    activeMarketQuoteRequests_.erase(marketRequest);
    const auto settle = [this, cacheOnly] {
      sendPendingMarketQuotes();
      if (!marketQuotePhaseBusy(cacheOnly)) {
        if (cacheOnly) {
          emit marketQuoteCacheSettled();
        } else {
          emit marketQuoteFetchSettled();
        }
      }
    };
    if (!message.value("ok").toBool()) {
      if (!cacheOnly) {
        emit marketQuoteRequestFailed(
            items,
            message.value("error").toString("market quote request failed"));
      }
      settle();
      return;
    }
    const QJsonValue dataValue = message.value("data");
    const QJsonObject data = dataValue.toObject();
    if (!dataValue.isObject() || !data.value("quotes").isArray() ||
        !data.value("missing").isArray()) {
      if (!cacheOnly) {
        emit marketQuoteRequestFailed(
            items, "daemon returned malformed market quotes");
      }
      settle();
      return;
    }
    const QJsonArray quotes = data.value("quotes").toArray();
    const QJsonArray missing = data.value("missing").toArray();
    if (!quotes.isEmpty() || (!cacheOnly && !missing.isEmpty())) {
      emit marketQuotesResolved(quotes, cacheOnly ? QJsonArray{} : missing);
    }
    settle();
    return;
  }

  const auto variantRequest = activeMarketVariantRequests_.find(id);
  if (variantRequest != activeMarketVariantRequests_.end()) {
    const MarketVariantRequest request = variantRequest.value();
    activeMarketVariantRequests_.erase(variantRequest);
    if (!message.value("ok").toBool()) {
      emit marketVariantQuoteFailed(request.item, request.filters,
                                    message.value("error").toString(
                                        "market variant quote request failed"));
    } else if (!message.value("data").isObject()) {
      emit marketVariantQuoteFailed(request.item, request.filters,
                                    "daemon returned malformed variant quote");
    } else {
      emit marketVariantQuoteReady(request.item, request.filters,
                                   message.value("data").toObject());
    }
    sendPendingMarketVariantQuotes();
    return;
  }

  const auto resolveRequest = activeMarketResolveRequests_.find(id);
  if (resolveRequest != activeMarketResolveRequests_.end()) {
    const MarketResolveRequest request = resolveRequest.value();
    activeMarketResolveRequests_.erase(resolveRequest);
    if (!message.value("ok").toBool()) {
      emit marketMatchesFailed(
          request.query,
          message.value("error").toString("market search request failed"));
      return;
    }
    const QJsonObject data = message.value("data").toObject();
    const QJsonArray resolutions = data.value("resolutions").toArray();
    if (!message.value("data").isObject() || resolutions.isEmpty() ||
        !resolutions.first().isObject() ||
        !resolutions.first().toObject().value("matches").isArray()) {
      emit marketMatchesFailed(request.query,
                               "daemon returned malformed market search data");
      return;
    }
    emit marketMatchesResolved(
        request.query,
        resolutions.first().toObject().value("matches").toArray());
    return;
  }

  const auto describeRequest = activeMarketDescriptionRequests_.find(id);
  if (describeRequest != activeMarketDescriptionRequests_.end()) {
    const QStringList items = describeRequest.value();
    activeMarketDescriptionRequests_.erase(describeRequest);
    if (!message.value("ok").toBool()) {
      emit marketItemDescribeFailed(
          items, message.value("error").toString("market item request failed"));
      sendPendingMarketDescriptions();
      return;
    }
    const QJsonValue dataValue = message.value("data");
    const QJsonObject data = dataValue.toObject();
    if (!dataValue.isObject() || !data.value("items").isArray() ||
        !data.value("missing").isArray()) {
      emit marketItemDescribeFailed(
          items, "daemon returned malformed market item data");
      sendPendingMarketDescriptions();
      return;
    }
    emit marketItemsDescribed(data.value("items").toArray(),
                              data.value("missing").toArray());
    sendPendingMarketDescriptions();
    return;
  }

  const auto accountRequest = activeMarketAccountRequests_.find(id);
  if (accountRequest != activeMarketAccountRequests_.end()) {
    const QString action = accountRequest.value();
    activeMarketAccountRequests_.erase(accountRequest);
    if (!message.value("ok").toBool()) {
      const QString error =
          message.value("error").toString("market account request failed");
      if (action == "presence") {
        emit marketPresenceFailed(error);
      } else {
        emit marketAccountFailed(action, error);
      }
      sendPendingMarketAccount();
      return;
    }
    const QJsonValue data = message.value("data");
    if (!data.isObject()) {
      if (action == "presence") {
        emit marketPresenceFailed("daemon returned malformed Market presence");
      } else {
        emit marketAccountFailed(
            action, "daemon returned malformed market account data");
      }
      sendPendingMarketAccount();
      return;
    }
    if (action == "presence") {
      emit marketPresenceReady(data.toObject(), true);
    } else {
      emit marketAccountReady(action, data.toObject());
      pendingMarketAccountSnapshot_ = false;
    }
    sendPendingMarketAccount();
    return;
  }

  const auto overframeRequest = activeOverframeAccountRequests_.find(id);
  if (overframeRequest != activeOverframeAccountRequests_.end()) {
    const QString action = overframeRequest.value();
    activeOverframeAccountRequests_.erase(overframeRequest);
    if (!message.value("ok").toBool()) {
      emit overframeAccountFailed(
          action,
          message.value("error").toString("Overframe account request failed"));
      sendPendingOverframeAccount();
      return;
    }
    const QJsonValue data = message.value("data");
    if (!data.isObject()) {
      emit overframeAccountFailed(
          action, "daemon returned malformed Overframe account data");
      sendPendingOverframeAccount();
      return;
    }
    emit overframeAccountReady(action, data.toObject());
    sendPendingOverframeAccount();
    return;
  }

  const auto buildRequest = activeBuildRequests_.find(id);
  if (buildRequest != activeBuildRequests_.end()) {
    const QJsonObject request = buildRequest.value();
    activeBuildRequests_.erase(buildRequest);
    if (!message.value("ok").toBool()) {
      emit buildSourceFailed(request, message.value("error").toString(
                                          "build source request failed"));
      sendPendingBuildRequests();
      return;
    }
    const QJsonValue data = message.value("data");
    if (!data.isObject()) {
      emit buildSourceFailed(request,
                             "daemon returned malformed build source data");
      sendPendingBuildRequests();
      return;
    }
    emit buildSourceReady(request, data.toObject());
    sendPendingBuildRequests();
    return;
  }

  const auto playerRequest = activePlayerViews_.find(id);
  if (playerRequest == activePlayerViews_.end()) {
    return;
  }
  const QString view = playerRequest.value();
  activePlayerViews_.erase(playerRequest);
  if (!message.value("ok").toBool()) {
    emit playerViewFailed(
        view, message.value("error").toString("player view request failed"));
    sendPendingPlayerViews();
    return;
  }
  const QJsonValue data = message.value("data");
  if (!data.isObject()) {
    emit playerViewFailed(view, "daemon returned malformed player view data");
    sendPendingPlayerViews();
    return;
  }
  emit playerViewReady(view, data.toObject());
  sendPendingPlayerViews();
}

void DaemonClient::handlePlayerSnapshot(const QJsonObject &data,
                                        const QString &source) {
  const qint64 revision = data.value("revision").toInteger(-1);
  if (revision < 0 ||
      (playerRevision_.has_value() && playerRevision_.value() == revision)) {
    return;
  }
  playerRevision_ = revision;
  emit playerDatasetChanged(revision, source);
}

void DaemonClient::sendHello() {
  write({
      {"op", "hello"},
      {"game", "warframe"},
      {"id", HelloRequestId},
      {"envelope", WFCLI_LOCAL_ENVELOPE},
      {"interfaces", guiInterfaces()},
      {"features", QJsonArray{"diagnostics.report"}},
      {"client", "wfgui"},
      {"version", WFCLI_VERSION},
      {"pid", QCoreApplication::applicationPid()},
      {"mode", "desktop"},
  });
}

void DaemonClient::sendPlayerSubscription() {
  write({{"op", "subscribe"},
         {"id", PlayerSubscriptionId},
         {"dataset", "player"},
         {"include_data", false}});
}

void DaemonClient::sendPendingPlayerViews() {
  if (!ready_ || pendingPlayerViews_.isEmpty()) {
    return;
  }
  const QSet<QString> pending = pendingPlayerViews_;
  for (const QString &view : pending) {
    if (activePlayerViews_.values().contains(view)) {
      continue;
    }
    pendingPlayerViews_.remove(view);
    const qint64 id = nextRequestId_++;
    activePlayerViews_.insert(id, view);
    QString operation = "mastery_view";
    if (view == "foundry") {
      operation = "foundry_view";
    } else if (view == "inventory") {
      operation = "inventory_view";
    } else if (view == "build_equipment") {
      operation = "build_equipment";
    }
    write({{"op", operation}, {"id", id}});
  }
}

void DaemonClient::sendPendingActivity() {
  if (!ready_ || !pendingActivity_ || activeActivityRequest_ != 0) {
    return;
  }
  pendingActivity_ = false;
  activeActivityRequest_ = nextRequestId_++;
  write({{"op", "activity_view"}, {"id", activeActivityRequest_}});
}

void DaemonClient::sendPendingNotificationSettings() {
  if (!ready_ || activeNotificationSettingsRequest_ != 0) {
    return;
  }
  if (desiredNotificationMode_.has_value()) {
    activeNotificationSettingsRequest_ = nextRequestId_++;
    activeNotificationRequestIsSet_ = true;
    sentNotificationMode_ = desiredNotificationMode_;
    write({{"op", "notification_settings_set"},
           {"id", activeNotificationSettingsRequest_},
           {"fissures", QJsonObject{{"mode", sentNotificationMode_.value()}}}});
    return;
  }
  if (pendingNotificationSettings_) {
    activeNotificationSettingsRequest_ = nextRequestId_++;
    activeNotificationRequestIsSet_ = false;
    write({{"op", "notification_settings"},
           {"id", activeNotificationSettingsRequest_}});
  }
}

void DaemonClient::sendPendingRequests() {
  if (!ready_ || pendingRelicRequests_.isEmpty()) {
    return;
  }
  const QHash<QString, RelicRequest> pending = pendingRelicRequests_;
  for (auto request = pending.cbegin(); request != pending.cend(); ++request) {
    if (relicRequestActive(request.key())) {
      continue;
    }
    pendingRelicRequests_.remove(request.key());
    const qint64 id = nextRequestId_++;
    activeRelicRequests_.insert(id, request.value());
    write({
        {"op", "relic_planner"},
        {"id", id},
        {"era", request->era},
        {"only_owned", false},
        {"fetch_prices", request->prices},
        {"limit", "all"},
    });
  }
}

void DaemonClient::sendPendingAssets() {
  if (!ready_) {
    return;
  }
  while (activeAssetRequests_.size() < MaxActiveAssetRequests &&
         !pendingAssetOrder_.isEmpty()) {
    QJsonArray batch;
    while (batch.size() < AssetBatchSize && !pendingAssetOrder_.isEmpty()) {
      const QString id = pendingAssetOrder_.takeFirst();
      const auto asset = pendingAssets_.find(id);
      if (asset == pendingAssets_.end() || assetRequestActive(id)) {
        continue;
      }
      batch.append(asset.value());
      pendingAssets_.erase(asset);
    }
    if (!batch.isEmpty()) {
      sendAssetBatch(batch);
    }
  }
}

void DaemonClient::sendPendingMarketQuotes() {
  if (!ready_) {
    return;
  }

  while (!pendingMarketCacheQuoteOrder_.isEmpty()) {
    QStringList items;
    while (items.size() < MarketCacheBatchSize &&
           !pendingMarketCacheQuoteOrder_.isEmpty()) {
      const QString item = pendingMarketCacheQuoteOrder_.takeFirst();
      if (!pendingMarketCacheQuotes_.remove(item) ||
          marketQuoteActive(item, true)) {
        continue;
      }
      items.append(item);
    }
    if (items.isEmpty()) {
      continue;
    }
    const qint64 id = nextRequestId_++;
    activeMarketQuoteRequests_.insert(
        id, {.items = items, .refresh = false, .cacheOnly = true});
    write({{"op", "market_quote"},
           {"id", id},
           {"items", QJsonArray::fromStringList(items)},
           {"ttl", MarketQuoteTtlSeconds},
           {"cache_only", true}});
  }

  int activeFetches = 0;
  for (const MarketQuoteRequest &request : activeMarketQuoteRequests_) {
    activeFetches += request.cacheOnly ? 0 : 1;
  }
  int remaining = pendingMarketQuoteOrder_.size();
  while (activeFetches < MaxActiveMarketQuoteRequests && remaining-- > 0 &&
         !pendingMarketQuoteOrder_.isEmpty()) {
    const QString item = pendingMarketQuoteOrder_.takeFirst();
    const auto pending = pendingMarketQuotes_.find(item);
    if (pending == pendingMarketQuotes_.end()) {
      continue;
    }
    if (marketQuoteActive(item, false)) {
      pendingMarketQuoteOrder_.append(item);
      continue;
    }
    const bool refresh = pending.value();
    pendingMarketQuotes_.erase(pending);
    const qint64 id = nextRequestId_++;
    activeMarketQuoteRequests_.insert(
        id, {.items = {item}, .refresh = refresh, .cacheOnly = false});
    write({{"op", "market_quote"},
           {"id", id},
           {"items", QJsonArray{item}},
           {"ttl", MarketQuoteTtlSeconds},
           {"refresh", refresh}});
    ++activeFetches;
  }
}

void DaemonClient::sendPendingMarketVariantQuotes() {
  constexpr int MaxActiveVariantQuotes = 4;
  if (!ready_) {
    return;
  }
  while (activeMarketVariantRequests_.size() < MaxActiveVariantQuotes &&
         !pendingMarketVariantRequests_.isEmpty()) {
    const MarketVariantRequest request =
        pendingMarketVariantRequests_.takeFirst();
    const qint64 id = nextRequestId_++;
    activeMarketVariantRequests_.insert(id, request);
    write({{"op", "market_quote_variant"},
           {"id", id},
           {"item", request.item},
           {"filters", request.filters},
           {"refresh", request.refresh}});
  }
}

void DaemonClient::sendPendingMarketResolve() {
  if (!ready_ || !pendingMarketResolve_.has_value()) {
    return;
  }
  const MarketResolveRequest request = *pendingMarketResolve_;
  pendingMarketResolve_.reset();
  const qint64 id = nextRequestId_++;
  activeMarketResolveRequests_.insert(id, request);
  write({{"op", "market_resolve"},
         {"id", id},
         {"labels", QJsonArray{request.query}},
         {"limit", request.limit}});
}

void DaemonClient::sendPendingMarketDescriptions() {
  if (!ready_) {
    return;
  }
  while (activeMarketDescriptionRequests_.size() <
             MaxActiveMarketDescriptionRequests &&
         !pendingMarketDescriptionOrder_.isEmpty()) {
    QStringList items;
    while (items.size() < MarketDescriptionBatchSize &&
           !pendingMarketDescriptionOrder_.isEmpty()) {
      const QString item = pendingMarketDescriptionOrder_.takeFirst();
      if (pendingMarketDescriptions_.remove(item)) {
        items.append(item);
      }
    }
    if (items.isEmpty()) {
      continue;
    }
    const qint64 id = nextRequestId_++;
    activeMarketDescriptionRequests_.insert(id, items);
    write({{"op", "market_describe"},
           {"id", id},
           {"items", QJsonArray::fromStringList(items)}});
  }
}

void DaemonClient::sendPendingMarketAccount() {
  if (!ready_) {
    return;
  }
  while (!pendingMarketAccountRequests_.isEmpty()) {
    MarketAccountRequest request = pendingMarketAccountRequests_.takeFirst();
    const qint64 id = nextRequestId_++;
    request.message.insert("id", id);
    activeMarketAccountRequests_.insert(id, request.action);
    write(request.message);
  }
  if (!pendingMarketAccountSnapshot_ ||
      !activeMarketAccountRequests_.isEmpty()) {
    return;
  }
  pendingMarketAccountSnapshot_ = false;
  const qint64 id = nextRequestId_++;
  activeMarketAccountRequests_.insert(id, "snapshot");
  write({{"op", "market_account"}, {"id", id}});
}

void DaemonClient::queueMarketAccountRequest(const QString &action,
                                             const QJsonObject &message) {
  pendingMarketAccountRequests_.append({.action = action, .message = message});
  sendPendingMarketAccount();
}

void DaemonClient::sendPendingOverframeAccount() {
  if (!ready_ || !activeOverframeAccountRequests_.isEmpty()) {
    return;
  }
  if (!pendingOverframeAccountRequests_.isEmpty()) {
    OverframeAccountRequest request =
        pendingOverframeAccountRequests_.takeFirst();
    const qint64 id = nextRequestId_++;
    request.message.insert("id", id);
    activeOverframeAccountRequests_.insert(id, request.action);
    write(request.message);
    return;
  }
  if (!pendingOverframeAccountSnapshot_) {
    return;
  }
  pendingOverframeAccountSnapshot_ = false;
  const qint64 id = nextRequestId_++;
  activeOverframeAccountRequests_.insert(id, "snapshot");
  write({{"op", "overframe_account"}, {"id", id}});
}

void DaemonClient::queueOverframeAccountRequest(const QString &action,
                                                const QJsonObject &message) {
  pendingOverframeAccountRequests_.append(
      {.action = action, .message = message});
  sendPendingOverframeAccount();
}

void DaemonClient::sendPendingBuildRequests() {
  while (ready_ && !pendingBuildRequests_.isEmpty() &&
         activeBuildRequests_.size() < MaxActiveBuildRequests) {
    QJsonObject request = pendingBuildRequests_.takeFirst();
    const qint64 id = nextRequestId_++;
    activeBuildRequests_.insert(id, request);
    request.insert("id", id);
    write(request);
  }
}

void DaemonClient::sendPendingDiagnostics() {
  if (!ready_ || !diagnosticsDirty_ || activeDiagnosticsRequest_ != 0 ||
      !negotiatedFeatures_.contains("diagnostics.report")) {
    return;
  }
  QStringList keys = resolutionIssues_.keys();
  keys.sort();
  QJsonArray issues;
  for (const QString &key : keys) {
    issues.append(resolutionIssues_.value(key));
  }
  diagnosticsDirty_ = false;
  activeDiagnosticsRequest_ = nextRequestId_++;
  write({{"op", "diagnostics_report"},
         {"id", activeDiagnosticsRequest_},
         {"issues", issues}});
}

void DaemonClient::queueBuildRequest(const QJsonObject &request) {
  if (!pendingBuildRequests_.contains(request) &&
      !activeBuildRequests_.values().contains(request)) {
    pendingBuildRequests_.append(request);
  }
  sendPendingBuildRequests();
}

void DaemonClient::sendAssetBatch(const QJsonArray &assets) {
  const qint64 id = nextRequestId_++;
  activeAssetRequests_.insert(id, assets);
  write({{"op", "asset_resolve"}, {"id", id}, {"assets", assets}});
}

void DaemonClient::sendPendingAssetCacheRequest() {
  if (!ready_ || activeAssetCacheRequest_ != 0) {
    return;
  }
  QString operation;
  if (pendingAssetCacheClear_) {
    pendingAssetCacheClear_ = false;
    activeAssetCacheClear_ = true;
    operation = "asset_cache_clear";
  } else if (pendingAssetCacheStatus_) {
    pendingAssetCacheStatus_ = false;
    activeAssetCacheClear_ = false;
    operation = "asset_cache_status";
  } else {
    return;
  }
  activeAssetCacheRequest_ = nextRequestId_++;
  write({{"op", operation}, {"id", activeAssetCacheRequest_}});
}

void DaemonClient::write(const QJsonObject &message) {
  QByteArray encoded = QJsonDocument(message).toJson(QJsonDocument::Compact);
  encoded.append('\n');
  const QString operation = message.value("op").toString();
  replies_.insert(
      message.value("id").toInteger(),
      {QDeadlineTimer(replyTimeout(operation), Qt::PreciseTimer), operation});
  armReplyDeadline();
  socket_->write(encoded);
  socket_->flush();
}

void DaemonClient::armReplyDeadline() {
  if (replies_.isEmpty()) {
    replyTimer_->stop();
    return;
  }
  const auto oldest =
      std::min_element(replies_.cbegin(), replies_.cend(),
                       [](const PendingReply &a, const PendingReply &b) {
                         return a.deadline < b.deadline;
                       });
  replyTimer_->start(
      static_cast<int>(qMax<qint64>(1, oldest->deadline.remainingTime())));
}

void DaemonClient::drainInput() {
  if (draining_) {
    return;
  }
  draining_ = true;
  QElapsedTimer elapsed;
  elapsed.start();
  int frames = 0;
  while (socket_->state() == QLocalSocket::ConnectedState &&
         frames < DispatchBatchSize && elapsed.elapsed() < DispatchBudgetMs) {
    const qsizetype newline = input_.indexOf('\n', scanOffset_);
    if (newline >= 0) {
      const QByteArray line = input_.left(newline).trimmed();
      input_.remove(0, newline + 1);
      scanOffset_ = 0;
      ++frames;
      if (!line.isEmpty()) {
        handleLine(line);
      }
      continue;
    }
    scanOffset_ = input_.size();
    if (input_.size() > MaximumFrameBytes) {
      socket_->abort();
      setStatus("Retinue daemon reply exceeds the 64 MiB frame limit");
      break;
    }
    if (socket_->bytesAvailable() == 0) {
      break;
    }
    input_.append(socket_->read(
        qMin(ReadChunkBytes, MaximumFrameBytes + 1 - input_.size())));
  }
  draining_ = false;
  if (socket_->state() == QLocalSocket::ConnectedState &&
      (socket_->bytesAvailable() > 0 || input_.size() > MaximumFrameBytes ||
       input_.indexOf('\n', scanOffset_) >= 0)) {
    inputTimer_->start(0);
  }
}

void DaemonClient::setConnected(bool connected) {
  if (connected_ == connected) {
    return;
  }
  connected_ = connected;
  emit connectionChanged();
}

void DaemonClient::setStatus(const QString &status) {
  if (status_ == status) {
    return;
  }
  status_ = status;
  emit statusChanged();
}

QString DaemonClient::socketPath() const { return wfgui::daemonSocketPath(); }

QString DaemonClient::wfcliCommand() const {
  const QString configured = qEnvironmentVariable("RETINUE_COMMAND");
  if (!configured.isEmpty()) {
    return configured;
  }

  QDir directory(QCoreApplication::applicationDirPath());
  for (;;) {
    const QFileInfo candidate(directory.filePath("retinue"));
    if (candidate.isFile() && candidate.isExecutable()) {
      return candidate.absoluteFilePath();
    }
    if (!directory.cdUp()) {
      break;
    }
  }
  return QStandardPaths::findExecutable("retinue");
}

QString DaemonClient::relicRequestKey(const RelicRequest &request) {
  return request.era + (request.prices ? QStringLiteral(":prices")
                                       : QStringLiteral(":metadata"));
}

bool DaemonClient::relicRequestActive(const QString &key) const {
  for (const RelicRequest &request : activeRelicRequests_) {
    if (relicRequestKey(request) == key) {
      return true;
    }
  }
  return false;
}

bool DaemonClient::assetRequestActive(const QString &id) const {
  for (const QJsonArray &assets : activeAssetRequests_) {
    for (const QJsonValue &asset : assets) {
      if (asset.toObject().value("id").toString() == id) {
        return true;
      }
    }
  }
  return false;
}

bool DaemonClient::marketQuoteActive(const QString &item,
                                     bool cacheOnly) const {
  for (const MarketQuoteRequest &request : activeMarketQuoteRequests_) {
    if (request.cacheOnly == cacheOnly && request.items.contains(item)) {
      return true;
    }
  }
  return false;
}

bool DaemonClient::marketQuotePhaseBusy(bool cacheOnly) const {
  if (cacheOnly ? !pendingMarketCacheQuotes_.isEmpty()
                : !pendingMarketQuotes_.isEmpty()) {
    return true;
  }
  return std::ranges::any_of(activeMarketQuoteRequests_,
                             [cacheOnly](const MarketQuoteRequest &request) {
                               return request.cacheOnly == cacheOnly;
                             });
}
