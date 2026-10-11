#include <QAbstractItemView>
#include <QCheckBox>
#include <QCompleter>
#include <QContextMenuEvent>
#include <QDateTime>
#include <QDirIterator>
#include <QFile>
#include <QHBoxLayout>
#include <QHelpEvent>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QLocalServer>
#include <QLocalSocket>
#include <QMenu>
#include <QPaintEvent>
#include <QPainter>
#include <QPushButton>
#include <QRegularExpression>
#include <QScopeGuard>
#include <QScrollBar>
#include <QSet>
#include <QTemporaryDir>
#include <QToolButton>
#include <QtTest>

#include <utility>

#ifdef Q_OS_LINUX
#include <sys/resource.h>
#endif

#include "activity_data.h"
#include "activity_rail_widget.h"
#include "animated_progress_bar.h"
#include "app_controller.h"
#include "compact_search.h"
#include "daemon_client.h"
#include "derivative_cache.h"
#include "display_scale.h"
#include "image_cache.h"
#include "inventory_card_layout.h"
#include "market_order_card.h"
#include "market_rail_widget.h"
#include "market_spin_box.h"
#include "player_item_grid_widget.h"
#include "player_item_model.h"
#include "relic_card_layout.h"
#include "relic_grid_widget.h"
#include "relic_model.h"
#include "relic_planner_widget.h"
#include "settings_widget.h"
#include "thumbnail_widget.h"
#include "tooltip.h"
#include "wfgui_paths.h"
#include "widget_capture.h"

class RelicModelTest final : public QObject {
  Q_OBJECT

  QTemporaryDir cacheRoot_;

private slots:
  void initTestCase() {
    QVERIFY(cacheRoot_.isValid());
    qputenv("XDG_CACHE_HOME", cacheRoot_.path().toUtf8());
  }
  void parsesRecommendations();
  void appliesResolvedAssets();
  void priceUpdatesPreserveRows();
  void filtersByName();
  void filtersByRewardName();
  void filtersRelicsByOwnershipLocally();
  void filtersRelicsByEraLocally();
  void requiemControlsWork();
  void filtersPlayerItemsLocally();
  void filtersPlayerItemFlags();
  void preservesUnknownInventoryVaultState();
  void preservesFavoriteOverrides();
  void appliesInventoryMarketQuotes();
  void countsOwnedMarketVariants();
  void sortsInventoryLocally();
  void appliesMasteryAcquisitionQuotes();
  void sortsMasteryRecommendations();
  void keepsMasteryRowsStableWhilePricesLoad();
  void keepsInventoryRowsStableWhilePricesLoad();
  void invalidatesCachedComponentAssets();
  void batchesContiguousAssetUpdates();
  void rejectsMalformedPayload();
  void cardLayoutUsesConstraints();
  void inventoryCardLayoutUsesConstraints();
  void gridLayoutUsesStableBreakpoints();
  void playerGridUsesAvailableColumns();
  void foundryGridUsesCompactCards();
  void foundryBlueprintAvailabilityUsesBackgroundState();
  void standardTooltipsUseLocalCoordinates();
  void foundryStatusBadgesHaveTooltips();
  void masteryComponentTooltipsUseLocalCoordinates();
  void componentClicksUseCounterpartyListings();
  void playerItemContextMenuRoutesAcrossPages();
  void relicContextMenuFiltersRewards();
  void masteryGridUsesCompactCards();
  void inventoryGridRequestsVisibleQuotes();
  void masteryGridRequestsComponentQuotes();
  void masteryGridRequestsAllComponentQuotes();
  void playerGridRequestsAssetsWhenShown();
  void subscribesToPlayerUpdatesAndCoalescesViews();
  void malformedReplyReconnectsWithoutReplayingMutations();
  void buildGroupsFollowPlayerRevisions();
  void updatesOnlyOlderDaemonContracts();
  void cacheMissDoesNotBecomeMarketMiss();
  void playerGridPreservesScrollAcrossResort();
  void busyProgressAnimates();
  void ownershipFilterKeepsVisibleRowsStable();
  void compactSearchExpandsOnClick();
  void normalizesUiScaleInFivePercentSteps();
  void thumbnailCacheDecodesJpeg();
  void reportsThumbnailDecodeFailures();
  void thumbnailCacheRespectsSizeAndDpr();
  void alignsFractionalDprThumbnailsToDevicePixels();
  void widgetThumbnailDecodeCompletesOffPaintPath();
  void thumbnailWorkIsBoundedWhenGuiFallsBehind();
  void thumbnailWidgetKeepsTintAndSurfaceAtDpr();
  void derivativeCacheTracksUpstreamIdentity();
  void renderingOlderAssetCannotRegressFreshness();
  void settingsExposeIndependentCacheControls();
  void pathReportIsStructured();
  void marketOrderCardUsesReferenceStructure();
  void masteryOwnedBadgeKeepsPaintMargin();
  void marketSearchKeyboardNavigation();
  void filtersExpiredFissures();
  void rightRailReachesBottom();
  void styleLayersAvoidImplicitSurfaces();
  void marketSpinBoxPaintsVisibleArrows();
  void capturesNamedUiTargets();
};

namespace {
class ThumbnailProbe final : public QWidget {
public:
  explicit ThumbnailProbe(QString path, QRect dirtyRegion = {})
      : ThumbnailProbe(wfgui::AssetRef::embedded(path, path), dirtyRegion) {}

  explicit ThumbnailProbe(wfgui::AssetRef asset, QRect dirtyRegion = {})
      : asset_(std::move(asset)), dirtyRegion_(dirtyRegion) {}

  const QPixmap &thumbnail() const { return thumbnail_; }
  QRect resolvedPaintRegion() const { return resolvedPaintRegion_; }

protected:
  void paintEvent(QPaintEvent *event) override {
    const bool unresolved = thumbnail_.isNull();
    QPainter painter(this);
    thumbnail_ =
        wfgui::cachedThumbnail(painter, asset_, QSize(20, 20), dirtyRegion_);
    if (unresolved && !thumbnail_.isNull()) {
      resolvedPaintRegion_ = event->region().boundingRect();
    }
  }

private:
  wfgui::AssetRef asset_;
  QRect dirtyRegion_;
  QRect resolvedPaintRegion_;
  QPixmap thumbnail_;
};

QJsonObject recommendations() {
  return {
      {"trace_count", 1842},
      {"items",
       QJsonArray{
           QJsonObject{
               {"name", "Axi A1 Intact"},
               {"era", "axi"},
               {"id", "relic-group:Axi A1 Relic"},
               {"amount_owned", 2},
               {"vaulted", false},
               {"favorite", true},
               {"asset", QJsonObject{{"id", "relic:axi-a1"}}},
               {"expected_platinum", 19},
               {"expected_ducats", 95},
               {"price_complete", true},
               {"refinements",
                QJsonArray{QJsonObject{{"refinement", "Intact"},
                                       {"amount_owned", 2},
                                       {"expected_platinum", 19},
                                       {"expected_ducats", 95},
                                       {"price_complete", true}},
                           QJsonObject{{"refinement", "Radiant"},
                                       {"amount_owned", 0},
                                       {"expected_platinum", 31},
                                       {"expected_ducats", 112},
                                       {"price_complete", true}}}},
               {"rewards",
                QJsonArray{QJsonObject{
                    {"name", "Saryn Prime Chassis"},
                    {"rarity", "Rare"},
                    {"platinum", 42},
                    {"ducats", 100},
                    {"owned", 1},
                    {"chance", 2.0},
                    {"asset", QJsonObject{{"id", "market:saryn_chassis"}}}}}}},
           QJsonObject{{"name", "Lith B1 Intact"},
                       {"era", "lith"},
                       {"amount_owned", 1},
                       {"vaulted", true},
                       {"favorite", false},
                       {"expected_platinum", QJsonValue(QJsonValue::Null)},
                       {"expected_ducats", 70}},
       }},
  };
}

QJsonObject localContractReply() {
  return {
      {"id", 1},
      {"ok", true},
      {"compatible", true},
      {"envelope", WFCLI_LOCAL_ENVELOPE},
      {"interfaces",
       QJsonObject{{"datasets", WFCLI_INTERFACE_DATASETS},
                   {"player", WFCLI_INTERFACE_PLAYER},
                   {"worldstate", WFCLI_INTERFACE_WORLDSTATE},
                   {"notifications", WFCLI_INTERFACE_NOTIFICATIONS},
                   {"market", WFCLI_INTERFACE_MARKET},
                   {"overframe", WFCLI_INTERFACE_OVERFRAME},
                   {"relics", WFCLI_INTERFACE_RELICS},
                   {"assets", WFCLI_INTERFACE_ASSETS},
                   {"builds", WFCLI_INTERFACE_BUILDS},
                   {"diagnostics", WFCLI_INTERFACE_DIAGNOSTICS}}},
      {"features", QJsonArray{"diagnostics.report"}},
  };
}
} // namespace

void RelicModelTest::parsesRecommendations() {
  RelicModel model;
  QString error;

  QVERIFY(model.replace(recommendations(), &error));
  QVERIFY(error.isEmpty());
  QCOMPARE(model.rowCount(), 2);
  QCOMPARE(model.traceCount(), 1842);

  const QModelIndex first = model.index(0);
  QCOMPARE(model.data(first, RelicModel::NameRole).toString(),
           QString("Axi A1 Intact"));
  QCOMPARE(model.data(first, RelicModel::AmountOwnedRole).toInt(), 2);
  QCOMPARE(model.data(first, RelicModel::HasPriceRole).toBool(), true);
  QCOMPARE(model.data(first, RelicModel::RefinementsRole).toList().size(), 2);
  QCOMPARE(model.data(first, RelicModel::RewardsRole).toList().size(), 1);

  const QModelIndex second = model.index(1);
  QCOMPARE(model.data(second, RelicModel::VaultedRole).toBool(), true);
  QCOMPARE(model.data(second, RelicModel::HasPriceRole).toBool(), false);
}

void RelicModelTest::appliesResolvedAssets() {
  RelicModel model;
  QVERIFY(model.replace(recommendations()));
  QSignalSpy changes(&model, &QAbstractItemModel::dataChanged);
  model.applyAssets(
      {{"relic:axi-a1",
        wfgui::AssetRef::embedded("relic:axi-a1", "/tmp/relic.png")},
       {"market:saryn_chassis",
        wfgui::AssetRef::embedded("market:saryn_chassis", "/tmp/reward.png")}});
  QCOMPARE(changes.count(), 1);
  QCOMPARE(qvariant_cast<QModelIndex>(changes.at(0).at(0)).row(), 0);
  QCOMPARE(qvariant_cast<QModelIndex>(changes.at(0).at(1)).row(), 0);

  const QModelIndex first = model.index(0);
  QCOMPARE(model.data(first, RelicModel::RelicImageRole).toString(),
           QString("/tmp/relic.png"));
  const QVariantMap reward =
      model.data(first, RelicModel::RewardsRole).toList().front().toMap();
  QCOMPARE(reward.value("image").toString(), QString("/tmp/reward.png"));
}

void RelicModelTest::priceUpdatesPreserveRows() {
  RelicModel model;
  QVERIFY(model.replace(recommendations()));
  QSignalSpy resets(&model, &QAbstractItemModel::modelReset);
  QSignalSpy changes(&model, &QAbstractItemModel::dataChanged);

  QJsonObject priced = recommendations();
  QJsonArray items = priced.value("items").toArray();
  QJsonObject first = items.at(0).toObject();
  first.insert("expected_platinum", 50);
  priced.insert("items", QJsonArray{items.at(1), first});

  QVERIFY(model.replace(priced));
  QCOMPARE(resets.count(), 0);
  QCOMPARE(changes.count(), 1);
  QCOMPARE(model.data(model.index(0), RelicModel::NameRole).toString(),
           QString("Axi A1 Intact"));
  QCOMPARE(model.data(model.index(0), RelicModel::ExpectedPlatinumRole).toInt(),
           50);
}

void RelicModelTest::filtersByName() {
  RelicModel model;
  QVERIFY(model.replace(recommendations()));
  RelicFilterModel filter;
  filter.setSourceModel(&model);

  filter.setFilterText("b1");
  QCOMPARE(filter.rowCount(), 1);
  QCOMPARE(filter.data(filter.index(0, 0), RelicModel::NameRole).toString(),
           QString("Lith B1 Intact"));
}

void RelicModelTest::filtersByRewardName() {
  RelicModel model;
  QVERIFY(model.replace(recommendations()));
  RelicFilterModel filter;
  filter.setSourceModel(&model);

  filter.setFilterText("saryn prime chassis");
  QCOMPARE(filter.rowCount(), 1);
  QCOMPARE(filter.data(filter.index(0, 0), RelicModel::NameRole).toString(),
           QString("Axi A1 Intact"));
  const QVariantMap reward =
      filter.data(filter.index(0, 0), RelicModel::RewardsRole)
          .toList()
          .front()
          .toMap();
  QVERIFY(reward.value("search_match").toBool());

  filter.setFilterText("axi");
  const QVariantMap nameMatch =
      filter.data(filter.index(0, 0), RelicModel::RewardsRole)
          .toList()
          .front()
          .toMap();
  QVERIFY(!nameMatch.value("search_match").toBool());
}

void RelicModelTest::filtersRelicsByOwnershipLocally() {
  RelicModel model;
  QVERIFY(model.replace(recommendations()));
  QJsonObject data = recommendations();
  QJsonArray items = data.value("items").toArray();
  QJsonObject unowned = items.at(1).toObject();
  unowned.insert("amount_owned", 0);
  items.replace(1, unowned);
  data.insert("items", items);
  QVERIFY(model.replace(data));

  RelicFilterModel filter;
  filter.setSourceModel(&model);
  QCOMPARE(filter.rowCount(), 1);
  const QVariantList ownedRefinements =
      filter.data(filter.index(0, 0), RelicModel::RefinementsRole).toList();
  QCOMPARE(ownedRefinements.size(), 1);
  QCOMPARE(ownedRefinements.front().toMap().value("name").toString(),
           QString("Intact"));
  filter.setOnlyOwned(false);
  QCOMPARE(filter.rowCount(), 2);
  QCOMPARE(filter.data(filter.index(0, 0), RelicModel::RefinementsRole)
               .toList()
               .size(),
           2);
}

void RelicModelTest::filtersRelicsByEraLocally() {
  RelicModel model;
  QVERIFY(model.replace(recommendations()));
  RelicFilterModel filter;
  filter.setSourceModel(&model);

  filter.setEra("lith");
  QCOMPARE(filter.rowCount(), 1);
  QCOMPARE(filter.data(filter.index(0, 0), RelicModel::NameRole).toString(),
           QString("Lith B1 Intact"));
  filter.setEra("all");
  QCOMPARE(filter.rowCount(), 2);
}

void RelicModelTest::requiemControlsWork() {
  QTemporaryDir directory;
  QVERIFY(directory.isValid());
  QLocalServer server;
  QVERIFY(server.listen(directory.filePath("wfdaemon.sock")));
  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", server.fullServerName().toUtf8());
  const auto restoreSocket = qScopeGuard([oldSocket] {
    if (oldSocket.isNull()) {
      qunsetenv("RETINUE_DAEMON_SOCKET");
    } else {
      qputenv("RETINUE_DAEMON_SOCKET", oldSocket);
    }
  });

  AppController controller;
  QTRY_VERIFY(server.hasPendingConnections());
  QVERIFY(server.nextPendingConnection());

  RelicPlannerWidget planner(&controller);
  ActivityRailWidget rail(&controller);
  connect(&rail, &ActivityRailWidget::relicEraRequested, &planner,
          &RelicPlannerWidget::showEra);
  planner.show();
  rail.show();

  QPushButton *requiem = nullptr;
  for (QPushButton *button : planner.findChildren<QPushButton *>()) {
    if (button->property("era").toString() == "requiem") {
      requiem = button;
      break;
    }
  }
  QVERIFY(requiem);
  QVERIFY(!requiem->icon().isNull());
  QTest::mouseClick(requiem, Qt::LeftButton);
  QCOMPARE(controller.selectedEra(), QString("requiem"));
  QVERIFY(requiem->isChecked());

  controller.selectEra("axi");
  auto *daemon = controller.findChild<DaemonClient *>();
  QVERIFY(daemon);
  QSignalSpy requested(&rail, &ActivityRailWidget::relicEraRequested);
  daemon->activityReady(QJsonObject{{
      "fissures",
      QJsonArray{QJsonObject{
          {"tier", "Requiem"},
          {"mission", "Survival"},
          {"node", "Taveuni (Kuva Fortress)"},
          {"hard", false},
          {"expiry",
           QDateTime::currentDateTimeUtc().addSecs(300).toString(Qt::ISODate)},
      }},
  }});
  auto cards = rail.findChildren<QWidget *>("fissureGroup");
  QTRY_COMPARE(cards.size(), 1);
  QWidget *card = cards.front();
  QTest::mouseClick(card, Qt::LeftButton, Qt::NoModifier,
                    QPoint(card->width() - 8, card->height() / 2));
  QCOMPARE(requested.size(), 1);
  QCOMPARE(requested.takeFirst().at(0).toString(), QString("requiem"));
  QCOMPARE(controller.selectedEra(), QString("requiem"));
  QVERIFY(requiem->isChecked());

  auto *filter = qobject_cast<RelicFilterModel *>(controller.relics());
  QVERIFY(filter);
  QCOMPARE(filter->era(), QString("requiem"));
}

void RelicModelTest::filtersPlayerItemsLocally() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{
           QJsonObject{{"id", "frame"},
                       {"name", "Test Prime"},
                       {"group", "warframes"},
                       {"mastered", false},
                       {"owned", true},
                       {"rank", 0},
                       {"from_relics", true},
                       {"buyable", true},
                       {"components",
                        QJsonArray{QJsonObject{
                            {"name", "Chassis"},
                            {"market_name", "Test Prime Chassis Blueprint"},
                        }}}},
           QJsonObject{{"id", "weapon"},
                       {"name", "Test Rifle"},
                       {"group", "weapons"},
                       {"mastered", true},
                       {"from_relics", false},
                       {"buyable", false}},
           QJsonObject{{"id", "unknown"},
                       {"name", "No Recipe Item"},
                       {"group", "weapons"},
                       {"mastered", false},
                       {"owned", false},
                       {"rank", 0},
                       {"has_recipe", false},
                       {"missing_parts", 0}},
       }},
  }));
  PlayerItemFilterModel filter;
  filter.setSourceModel(&model);
  filter.setMode("easy");
  QCOMPARE(filter.rowCount(), 1);
  filter.setMode("relics");
  QCOMPARE(filter.rowCount(), 1);
  filter.setGroup("weapons");
  QCOMPARE(filter.rowCount(), 0);
  filter.setGroup("all");
  filter.setMode("all");
  filter.setText("chassis blueprint");
  QCOMPARE(filter.rowCount(), 1);
  const QVariantMap component =
      filter.data(filter.index(0, 0), PlayerItemModel::ComponentsRole)
          .toList()
          .front()
          .toMap();
  QVERIFY(component.value("search_match").toBool());
}

void RelicModelTest::filtersPlayerItemFlags() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{
           QJsonObject{{"id", "prime"},
                       {"name", "Prime Item"},
                       {"mastered", true},
                       {"owned", true},
                       {"is_prime", true},
                       {"vaulted", true},
                       {"subsumed", true},
                       {"ready_to_build", false}},
           QJsonObject{{"id", "ready"},
                       {"name", "Ready Item"},
                       {"quantity", 1},
                       {"mastered", false},
                       {"owned", false},
                       {"is_prime", false},
                       {"ready_to_build", true}},
       }},
  }));
  PlayerItemFilterModel filter;
  filter.setSourceModel(&model);

  filter.setFlag("prime", 1);
  QCOMPARE(filter.rowCount(), 1);
  QCOMPARE(
      filter.data(filter.index(0, 0), PlayerItemModel::NameRole).toString(),
      QString("Prime Item"));
  filter.setFlag("prime", -1);
  filter.setFlag("ready", 1);
  QCOMPARE(filter.rowCount(), 1);
  QCOMPARE(
      filter.data(filter.index(0, 0), PlayerItemModel::NameRole).toString(),
      QString("Ready Item"));
  filter.setFlag("ready", -1);
  QCOMPARE(filter.rowCount(), 2);
  filter.setFlag("vaulted", 1);
  QCOMPARE(filter.rowCount(), 1);
  filter.setFlag("vaulted", -1);
  filter.setFlag("duplicate", 1);
  QCOMPARE(filter.rowCount(), 0);
  filter.setFlag("duplicate", -1);
  const QModelIndex prime = model.index(0);
  QVERIFY(model.data(prime, PlayerItemModel::VaultedRole).toBool());
  QVERIFY(model.data(prime, PlayerItemModel::SubsumedRole).toBool());
}

void RelicModelTest::preservesUnknownInventoryVaultState() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{
           QJsonObject{
               {"id", "vaulted"}, {"name", "Vaulted"}, {"vaulted", true}},
           QJsonObject{
               {"id", "unvaulted"}, {"name", "Unvaulted"}, {"vaulted", false}},
           QJsonObject{{"id", "unknown"},
                       {"name", "Unknown"},
                       {"vaulted", QJsonValue(QJsonValue::Null)}},
       }},
  }));
  QVERIFY(!model.data(model.index(2), PlayerItemModel::VaultedRole).isValid());

  PlayerItemFilterModel filter;
  filter.setSourceModel(&model);
  filter.setFlag("vaulted", 1);
  QCOMPARE(filter.rowCount(), 1);
  QCOMPARE(
      filter.data(filter.index(0, 0), PlayerItemModel::NameRole).toString(),
      QString("Vaulted"));
  filter.setFlag("vaulted", 0);
  QCOMPARE(filter.rowCount(), 1);
  QCOMPARE(
      filter.data(filter.index(0, 0), PlayerItemModel::NameRole).toString(),
      QString("Unvaulted"));
}

void RelicModelTest::preservesFavoriteOverrides() {
  PlayerItemModel model;
  const QJsonObject data = {
      {"items",
       QJsonArray{QJsonObject{
           {"id", "frame"}, {"name", "Test Prime"}, {"favorite", true}}}},
  };
  QVERIFY(model.replace(data));
  const QModelIndex item = model.index(0);
  QVERIFY(model.data(item, PlayerItemModel::FavoriteRole).toBool());

  QVERIFY(model.setData(item, false, PlayerItemModel::FavoriteRole));
  QVERIFY(!model.data(item, PlayerItemModel::FavoriteRole).toBool());
  QVERIFY(model.replace(data));
  QVERIFY(!model.data(model.index(0), PlayerItemModel::FavoriteRole).toBool());

  QVERIFY(model.setData(model.index(0), true, PlayerItemModel::FavoriteRole));
  PlayerItemFilterModel filter;
  filter.setSourceModel(&model);
  filter.setFlag("favorite", 1);
  QCOMPARE(filter.rowCount(), 1);
}

void RelicModelTest::appliesInventoryMarketQuotes() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items", QJsonArray{QJsonObject{{"id", "relic"},
                                       {"name", "Axi A20 Intact"},
                                       {"market_name", "Axi A20 Relic"},
                                       {"group", "relics"},
                                       {"quantity", 2},
                                       {"tradable", true}}}},
  }));
  const QModelIndex item = model.index(0);
  QCOMPARE(model.data(item, PlayerItemModel::MarketNameRole).toString(),
           QString("Axi A20 Relic"));
  QVERIFY(model.data(item, PlayerItemModel::SellableRole).toBool());
  QCOMPARE(model.data(item, PlayerItemModel::PriceStateRole).toString(),
           QString("loading"));
  QSignalSpy changes(&model, &QAbstractItemModel::dataChanged);

  model.applyMarketQuotes(
      QJsonArray{QJsonObject{
          {"item", "Axi A20 Relic"},
          {"slug", "axi_a20_relic"},
          {"quote", QJsonObject{{"lowest_sell", 17}, {"highest_buy", 14}}},
      }},
      {});
  QCOMPARE(model.data(item, PlayerItemModel::PlatinumRole).toInt(), 17);
  QCOMPARE(model.data(item, PlayerItemModel::BuyPlatinumRole).toInt(), 14);
  QCOMPARE(model.data(item, PlayerItemModel::PriceStateRole).toString(),
           QString("ready"));
  QCOMPARE(changes.count(), 1);

  model.markMarketUnavailable({"Axi A20 Relic"});
  QCOMPARE(model.data(item, PlayerItemModel::PriceStateRole).toString(),
           QString("ready"));

  QVERIFY(model.replace({
      {"items", QJsonArray{QJsonObject{{"id", "set"},
                                       {"name", "Test Prime Set"},
                                       {"group", "sets"},
                                       {"quantity", 0},
                                       {"tradable", true}}}},
  }));
  QVERIFY(!model.data(model.index(0), PlayerItemModel::SellableRole).toBool());
}

void RelicModelTest::countsOwnedMarketVariants() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{
           QJsonObject{{"id", "hush-0"},
                       {"name", "Hush"},
                       {"market_name", "Hush"},
                       {"group", "mods"},
                       {"rank", 0},
                       {"quantity", 31}},
           QJsonObject{{"id", "hush-1"},
                       {"name", "Hush"},
                       {"market_name", "Hush"},
                       {"group", "mods"},
                       {"rank", 1},
                       {"quantity", 2}},
       }},
  }));

  QCOMPARE(model.ownedQuantity("hush"), 33);
  QCOMPARE(model.ownedQuantity("Hush", 0), 31);
  QCOMPARE(model.ownedQuantity("Hush", 1), 2);
  QCOMPARE(model.ownedQuantity("Hush", 5), 0);
}

void RelicModelTest::sortsInventoryLocally() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{
           QJsonObject{{"id", "cheap"},
                       {"name", "Cheap Prime Part"},
                       {"quantity", 4},
                       {"ducats", 15},
                       {"tradable", true}},
           QJsonObject{{"id", "valuable"},
                       {"name", "Valuable Prime Part"},
                       {"quantity", 1},
                       {"ducats", 100},
                       {"tradable", true}},
       }},
  }));
  model.applyMarketQuotes(
      QJsonArray{
          QJsonObject{{"item", "Cheap Prime Part"},
                      {"quote", QJsonObject{{"lowest_sell", 3}}}},
          QJsonObject{{"item", "Valuable Prime Part"},
                      {"quote", QJsonObject{{"lowest_sell", 20}}}},
      },
      {});

  PlayerItemFilterModel filter;
  filter.setSourceModel(&model);
  filter.setSortMode("platinum");
  QCOMPARE(
      filter.data(filter.index(0, 0), PlayerItemModel::NameRole).toString(),
      QString("Cheap Prime Part"));
  filter.setSortAscending(false);
  QCOMPARE(
      filter.data(filter.index(0, 0), PlayerItemModel::NameRole).toString(),
      QString("Valuable Prime Part"));
  filter.setSortMode("amount");
  QCOMPARE(
      filter.data(filter.index(0, 0), PlayerItemModel::NameRole).toString(),
      QString("Cheap Prime Part"));
}

void RelicModelTest::appliesMasteryAcquisitionQuotes() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{QJsonObject{
           {"id", "frame"},
           {"name", "Test Prime"},
           {"relic_probability", 0.4375},
           {"components",
            QJsonArray{
                QJsonObject{{"name", "Test Prime Chassis"},
                            {"market_name", "Test Prime Chassis Blueprint"},
                            {"market_required", true},
                            {"required", 2},
                            {"owned", 1},
                            {"tradable", true}},
                QJsonObject{{"name", "Test Prime Systems"},
                            {"required", 1},
                            {"owned", 1},
                            {"tradable", true}},
            }},
       }}},
  }));
  const QModelIndex item = model.index(0);
  QCOMPARE(model.data(item, PlayerItemModel::RelicProbabilityRole).toDouble(),
           0.4375);
  QVERIFY(
      !model.data(item, PlayerItemModel::AcquisitionPlatinumRole).isValid());
  QCOMPARE(
      model.data(item, PlayerItemModel::AcquisitionPriceStateRole).toString(),
      QString("loading"));

  model.applyMarketQuotes(
      QJsonArray{QJsonObject{
          {"item", "Test Prime Chassis Blueprint"},
          {"quote", QJsonObject{{"lowest_sell", 7}, {"highest_buy", 5}}},
      }},
      {});
  QCOMPARE(model.data(item, PlayerItemModel::AcquisitionPlatinumRole).toInt(),
           7);
  QCOMPARE(
      model.data(item, PlayerItemModel::AcquisitionPriceStateRole).toString(),
      QString("ready"));
}

void RelicModelTest::sortsMasteryRecommendations() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{
           QJsonObject{{"id", "unlikely"},
                       {"name", "Unlikely Prime"},
                       {"mastered", false},
                       {"from_relics", true},
                       {"relic_probability", 0.1},
                       {"missing_parts", 1}},
           QJsonObject{{"id", "likely"},
                       {"name", "Likely Prime"},
                       {"mastered", false},
                       {"from_relics", true},
                       {"relic_probability", 0.75},
                       {"missing_parts", 2}},
       }},
  }));
  PlayerItemFilterModel filter;
  filter.setSourceModel(&model);
  filter.setMode("relics");
  QCOMPARE(
      filter.data(filter.index(0, 0), PlayerItemModel::NameRole).toString(),
      QString("Likely Prime"));
}

void RelicModelTest::keepsMasteryRowsStableWhilePricesLoad() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{
           QJsonObject{
               {"id", "alpha"},
               {"name", "Alpha Prime"},
               {"mastered", false},
               {"buyable", true},
               {"potential_xp", 3000},
               {"components", QJsonArray{QJsonObject{
                                  {"market_name", "Alpha Prime Blueprint"},
                                  {"market_required", true},
                                  {"required", 1},
                                  {"owned", 0},
                              }}},
           },
           QJsonObject{
               {"id", "beta"},
               {"name", "Beta Prime"},
               {"mastered", false},
               {"buyable", true},
               {"potential_xp", 6000},
               {"components", QJsonArray{QJsonObject{
                                  {"market_name", "Beta Prime Blueprint"},
                                  {"market_required", true},
                                  {"required", 1},
                                  {"owned", 0},
                              }}},
           },
       }},
  }));
  PlayerItemFilterModel filter;
  filter.setSourceModel(&model);
  filter.setPricesLoading(true);
  filter.setMode("platinum");

  QCOMPARE(filter.rowCount(), 2);
  QCOMPARE(filter.index(0, 0).data(PlayerItemModel::NameRole).toString(),
           QString("Alpha Prime"));
  QSignalSpy layoutChanges(&filter, &QAbstractItemModel::layoutChanged);
  model.applyMarketQuotes(
      QJsonArray{
          QJsonObject{
              {"item", "Alpha Prime Blueprint"},
              {"quote", QJsonObject{{"lowest_sell", 10}}},
          },
          QJsonObject{
              {"item", "Beta Prime Blueprint"},
              {"quote", QJsonObject{{"lowest_sell", 15}}},
          },
      },
      {});

  QCOMPARE(layoutChanges.count(), 0);
  QCOMPARE(filter.index(0, 0).data(PlayerItemModel::NameRole).toString(),
           QString("Alpha Prime"));
  filter.sort(0);
  QCOMPARE(filter.index(0, 0).data(PlayerItemModel::NameRole).toString(),
           QString("Beta Prime"));
  layoutChanges.clear();
  model.applyMarketQuotes(
      QJsonArray{
          QJsonObject{
              {"item", "Alpha Prime Blueprint"},
              {"quote", QJsonObject{{"lowest_sell", 5}}},
          },
          QJsonObject{
              {"item", "Beta Prime Blueprint"},
              {"quote", QJsonObject{{"lowest_sell", 20}}},
          },
      },
      {});

  QCOMPARE(filter.rowCount(), 2);
  QCOMPARE(filter.index(0, 0).data(PlayerItemModel::NameRole).toString(),
           QString("Beta Prime"));
  QCOMPARE(filter.index(1, 0).data(PlayerItemModel::NameRole).toString(),
           QString("Alpha Prime"));
  QCOMPARE(layoutChanges.count(), 0);

  filter.setPricesLoading(false);
  QCOMPARE(filter.rowCount(), 2);
  QCOMPARE(filter.index(0, 0).data(PlayerItemModel::NameRole).toString(),
           QString("Alpha Prime"));
  QCOMPARE(filter.index(1, 0).data(PlayerItemModel::NameRole).toString(),
           QString("Beta Prime"));
}

void RelicModelTest::keepsInventoryRowsStableWhilePricesLoad() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{
           QJsonObject{{"id", "alpha"},
                       {"name", "Alpha Prime Part"},
                       {"tradable", true},
                       {"ducats", 15}},
           QJsonObject{{"id", "beta"},
                       {"name", "Beta Prime Part"},
                       {"tradable", true},
                       {"ducats", 45}},
       }},
  }));
  PlayerItemFilterModel filter;
  filter.setSourceModel(&model);
  filter.setPricesLoading(true);
  filter.setSortMode("platinum");

  QSignalSpy layoutChanges(&filter, &QAbstractItemModel::layoutChanged);
  model.applyMarketQuotes(QJsonArray{QJsonObject{
                              {"item", "Beta Prime Part"},
                              {"quote", QJsonObject{{"lowest_sell", 2}}},
                          }},
                          {});

  QCOMPARE(layoutChanges.count(), 0);
  QCOMPARE(filter.index(0, 0).data(PlayerItemModel::IdRole).toString(),
           QString("alpha"));

  model.applyMarketQuotes(QJsonArray{QJsonObject{
                              {"item", "Alpha Prime Part"},
                              {"quote", QJsonObject{{"lowest_sell", 20}}},
                          }},
                          {});
  filter.setPricesLoading(false);
  QCOMPARE(filter.index(0, 0).data(PlayerItemModel::IdRole).toString(),
           QString("beta"));
}

void RelicModelTest::invalidatesCachedComponentAssets() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{QJsonObject{
           {"id", "set"},
           {"name", "Test Set"},
           {"components",
            QJsonArray{QJsonObject{{"name", "Test Part"},
                                   {"asset", QJsonObject{{"id", "part"}}}}}},
       }}},
  }));
  const QModelIndex item = model.index(0);

  model.setAssets(
      {{"part", wfgui::AssetRef::embedded("part", "/tmp/first.png")}});
  QCOMPARE(model.data(item, PlayerItemModel::ComponentsRole)
               .toList()
               .front()
               .toMap()
               .value("image")
               .toString(),
           QString("/tmp/first.png"));

  model.setAssets(
      {{"part", wfgui::AssetRef::embedded("part", "/tmp/second.png")}});
  QCOMPARE(model.data(item, PlayerItemModel::ComponentsRole)
               .toList()
               .front()
               .toMap()
               .value("image")
               .toString(),
           QString("/tmp/second.png"));

  model.setAssets({});
  QVERIFY(model.data(item, PlayerItemModel::ComponentsRole)
              .toList()
              .front()
              .toMap()
              .value("image")
              .toString()
              .isEmpty());
}

void RelicModelTest::batchesContiguousAssetUpdates() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{
           QJsonObject{{"id", "first"},
                       {"name", "First"},
                       {"asset", QJsonObject{{"id", "shared"}}}},
           QJsonObject{{"id", "second"},
                       {"name", "Second"},
                       {"asset", QJsonObject{{"id", "shared"}}}},
           QJsonObject{{"id", "third"},
                       {"name", "Third"},
                       {"asset", QJsonObject{{"id", "other"}}}},
       }},
  }));

  QSignalSpy changes(&model, &QAbstractItemModel::dataChanged);
  model.applyAssets(
      {{"shared", wfgui::AssetRef::embedded("shared", "/tmp/shared.png")}});

  QCOMPARE(changes.count(), 1);
  QCOMPARE(qvariant_cast<QModelIndex>(changes.at(0).at(0)).row(), 0);
  QCOMPARE(qvariant_cast<QModelIndex>(changes.at(0).at(1)).row(), 1);
}

void RelicModelTest::rejectsMalformedPayload() {
  RelicModel model;
  QString error;
  QVERIFY(!model.replace({{"trace_count", 10}}, &error));
  QVERIFY(!error.isEmpty());
  QCOMPARE(model.rowCount(), 0);
}

void RelicModelTest::cardLayoutUsesConstraints() {
  const auto narrow = wfgui::RelicCardLayout::calculate({0, 0, 420, 108}, 4, 6);
  const auto wide = wfgui::RelicCardLayout::calculate({0, 0, 620, 108}, 4, 6);
  const auto dense =
      wfgui::RelicCardLayout::calculate({0, 0, 472, 122}, 4, 6, 1.125);

  QCOMPARE(narrow.image.width(), 80);
  QCOMPARE(wide.image.width(), 80);
  QCOMPARE(narrow.rewards.width(), 127);
  QCOMPARE(wide.rewards.width(), 127);
  QCOMPARE(narrow.middle.width(), 173);
  QCOMPARE(wide.middle.width(), 220);
  QCOMPARE(narrow.refinementRows.size(), 4);
  QCOMPARE(narrow.rewardCells.size(), 6);
  QCOMPARE(narrow.rewardCells.front().size(), QSize(37, 37));
  QCOMPARE(dense.image.width(), 90);
  QCOMPARE(dense.rewardCells.front().size(), QSize(42, 42));
  QVERIFY(narrow.image.right() < narrow.middle.left());
  QVERIFY(narrow.middle.right() < narrow.rewards.left());
  QVERIFY(wide.image.left() > narrow.image.left());
  QCOMPARE(narrow.platinumHeader.center().x(),
           narrow.refinementRows.front().platinum.center().x());
  QCOMPARE(narrow.ducatHeader.center().x(),
           narrow.refinementRows.front().ducats.center().x());
  QVERIFY(wide.title.width() > narrow.title.width());
}

void RelicModelTest::inventoryCardLayoutUsesConstraints() {
  const auto card =
      wfgui::InventoryCardLayout::calculate({0, 0, 408, 118}, false, 0);
  const auto set =
      wfgui::InventoryCardLayout::calculate({0, 0, 408, 118}, true, 6);
  const auto scaled =
      wfgui::InventoryCardLayout::calculate({0, 0, 459, 133}, false, 0, 1.125);

  QCOMPARE(card.image.width(), 90);
  QCOMPARE(card.ducats.height(), 24);
  QCOMPARE(card.title.height(), 27);
  QCOMPARE(card.sell.height(), 35);
  QCOMPARE(card.buy.height(), 35);
  QCOMPARE(card.buy.left() - card.sell.right() - 1, 11);
  QVERIFY(card.status.bottom() < card.sell.top());
  QCOMPARE(set.components.size(), 6);
  QCOMPARE(set.components.front().size(), QSize(35, 35));
  QVERIFY(set.sell.width() >= 90);
  QVERIFY(set.sell.width() <= 114);
  QVERIFY(set.buy.isNull());
  QCOMPARE(scaled.image.width(), 101);
  QCOMPARE(scaled.sell.height(), 39);
}

void RelicModelTest::gridLayoutUsesStableBreakpoints() {
  QCOMPARE(wfgui::relicGridColumns(399), 1);
  QCOMPARE(wfgui::relicGridColumns(1000), 2);
  QCOMPARE(wfgui::relicGridColumns(1900), 4);
  QCOMPARE(wfgui::relicGridColumns(2280), 4);
  QCOMPARE(wfgui::relicGridColumns(2700), 5);
  QCOMPARE(wfgui::relicGridColumns(2280, 1.125), 4);
}

void RelicModelTest::playerGridUsesAvailableColumns() {
  QJsonArray items;
  for (int row = 0; row < 12; ++row) {
    items.append(QJsonObject{{"id", QString("item-%1").arg(row)},
                             {"name", QString("Item %1").arg(row)},
                             {"group", "parts"}});
  }
  PlayerItemModel model;
  QVERIFY(model.replace({{"items", items}}));

  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Inventory);
  grid.setModel(&model);
  grid.resize(2296, 800);
  grid.show();
  QCoreApplication::processEvents();

  QVERIFY(grid.gridSize().width() > 0);
  QCOMPARE(grid.gridSize().height(), 138);
  const int columns = grid.viewport()->width() / grid.gridSize().width();
  QVERIFY(columns >= 5);
  QTRY_COMPARE(grid.visualRect(model.index(columns - 1)).y(),
               grid.visualRect(model.index(0)).y());
  QTRY_VERIFY(grid.visualRect(model.index(columns)).y() >
              grid.visualRect(model.index(0)).y());
}

void RelicModelTest::masteryGridUsesCompactCards() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{QJsonObject{
           {"id", "frame"}, {"name", "Test Prime"}, {"group", "warframes"}}}},
  }));

  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Mastery);
  grid.setModel(&model);
  grid.resize(800, 600);
  grid.show();
  QCoreApplication::processEvents();

  QCOMPARE(grid.gridSize().height(), 99);
}

void RelicModelTest::playerGridRequestsAssetsWhenShown() {
  PlayerItemModel model;
  QVERIFY(model.replace(
      {{"items",
        QJsonArray{QJsonObject{
            {"id", "test-item"},
            {"name", "Test Item"},
            {"group", "parts"},
            {"asset", QJsonObject{{"id", "asset:test-item"},
                                  {"source", "wfcd"},
                                  {"image_name", "test-item.png"}}}}}}}));

  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Inventory);
  grid.setModel(&model);
  grid.resize(800, 600);
  QSignalSpy assets(&grid, &PlayerItemGridWidget::assetsNeeded);
  grid.show();
  QTRY_VERIFY_WITH_TIMEOUT(!assets.isEmpty(), 1000);

  grid.hide();
  QCoreApplication::processEvents();
  assets.clear();
  grid.show();
  QTRY_VERIFY_WITH_TIMEOUT(!assets.isEmpty(), 1000);
}

void RelicModelTest::foundryGridUsesCompactCards() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{QJsonObject{
           {"id", "test-prime"},
           {"name", "Test Prime"},
           {"group", "warframe"},
           {"owned", true},
           {"pending", true},
           {"mastered", true},
           {"mastery_requirement", 10},
           {"is_prime", true},
           {"vaulted", true},
           {"subsumed", true},
           {"components",
            QJsonArray{
                QJsonObject{
                    {"name", "Blueprint"}, {"required", 1}, {"owned", 1}},
                QJsonObject{{"name", "Chassis"}, {"required", 1}, {"owned", 1}},
                QJsonObject{{"name", "Systems"}, {"required", 1}, {"owned", 0}},
            }},
       }}},
  }));

  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Foundry);
  grid.setModel(&model);
  grid.resize(320, 400);
  grid.show();
  QCoreApplication::processEvents();

  QCOMPARE(grid.gridSize().height(), 150);
  QString error;
  const QPixmap card = wfgui::grabCaptureTarget("foundry.grid.item", 0, &error);
  QVERIFY2(!card.isNull(), qPrintable(error));
  QCOMPARE(card.deviceIndependentSize().height(), 150.0);
}

void RelicModelTest::foundryBlueprintAvailabilityUsesBackgroundState() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{QJsonObject{
           {"id", "test-frame"},
           {"name", "Test Frame"},
           {"group", "warframe"},
           {"components",
            QJsonArray{QJsonObject{{"name", "Chassis"},
                                   {"required", 1},
                                   {"owned", 0},
                                   {"blueprint_owned", 2}}}},
       }}},
  }));

  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Foundry);
  grid.setModel(&model);
  grid.resize(320, 400);
  grid.show();
  QCoreApplication::processEvents();

  const QRect item = grid.visualRect(model.index(0));
  const QRect card = item.adjusted(4, 4, -4, -4);
  const QRect content = card.adjusted(8, 8, -8, -8);
  constexpr int componentSize = 39;
  constexpr int componentGap = 8;
  constexpr int componentLayoutWidth =
      3 * componentSize + 2 * componentGap;
  const int componentLeft = content.right() - componentLayoutWidth + 1 +
                            (componentLayoutWidth - componentSize) / 2;
  const QRect body(content.left(), content.top() + 33, content.width(), 86);
  const QRect component(componentLeft,
                        body.center().y() - componentSize / 2, componentSize,
                        componentSize);
  const QColor fill = grid.viewport()->grab().toImage().pixelColor(
      component.center());
  QVERIFY(fill.green() > fill.red());
  QVERIFY(fill.green() > fill.blue());

  wfgui::hideTooltip();
  QHelpEvent event(QEvent::ToolTip, component.center(),
                   grid.viewport()->mapToGlobal(component.center()));
  QApplication::sendEvent(grid.viewport(), &event);
  QCoreApplication::processEvents();
  QLabel *tip =
      grid.findChild<QLabel *>("wfguiTooltip", Qt::FindDirectChildrenOnly);
  QVERIFY(tip);
  QCOMPARE(tip->text(), QString("Chassis\nOwned: 0/1\nBlueprints: 2"));
  wfgui::hideTooltip();
}

void RelicModelTest::standardTooltipsUseLocalCoordinates() {
  wfgui::installTooltipHandling(*qApp);
  QWidget host;
  QToolButton button(&host);
  button.setToolTip("Locally positioned");
  button.setGeometry(40, 30, 80, 30);
  host.resize(240, 140);
  host.show();
  QCoreApplication::processEvents();

  const QPoint local(10, 10);
  const QPoint expected = button.mapTo(&host, local);
  QHelpEvent event(QEvent::ToolTip, local,
                   button.mapToGlobal(local) + QPoint(10000, 10000));
  QApplication::sendEvent(&button, &event);
  QCoreApplication::processEvents();

  QLabel *tip =
      host.findChild<QLabel *>("wfguiTooltip", Qt::FindDirectChildrenOnly);
  QVERIFY(tip != nullptr);
  QVERIFY(tip->isVisible());
  QVERIFY(!tip->isWindow());
  QCOMPARE(tip->text(), QString("Locally positioned"));
  const QRect vicinity(expected - QPoint(50, 50), QSize(100, 100));
  QVERIFY2(vicinity.intersects(tip->geometry()),
           qPrintable(QString("tooltip at %1,%2; expected near %3,%4")
                          .arg(tip->x())
                          .arg(tip->y())
                          .arg(expected.x())
                          .arg(expected.y())));
  wfgui::hideTooltip();
  QCoreApplication::processEvents();
}

void RelicModelTest::foundryStatusBadgesHaveTooltips() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items", QJsonArray{QJsonObject{{"id", "test-prime"},
                                       {"name", "Test Prime"},
                                       {"group", "warframe"},
                                       {"pending", true},
                                       {"mastered", true},
                                       {"mastery_requirement", 10},
                                       {"is_prime", true},
                                       {"vaulted", true},
                                       {"subsumed", true}}}},
  }));

  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Foundry);
  grid.setModel(&model);
  grid.resize(320, 400);
  grid.show();
  QCoreApplication::processEvents();

  const QRect item = grid.visualRect(model.index(0));
  const QRect card = item.adjusted(4, 4, -4, -4);
  const QRect content = card.adjusted(8, 8, -8, -8);
  const QRect image(content.left() + 3, card.bottom() - 105 + 1, 105, 105);
  const auto tooltipAt = [&grid](const QPoint &position) {
    wfgui::hideTooltip();
    const QPoint expected = grid.viewport()->mapTo(&grid, position);
    QCoreApplication::processEvents();
    QHelpEvent event(QEvent::ToolTip, position,
                     grid.viewport()->mapToGlobal(position) +
                         QPoint(10000, 10000));
    QApplication::sendEvent(grid.viewport(), &event);
    QCoreApplication::processEvents();
    QLabel *tip =
        grid.findChild<QLabel *>("wfguiTooltip", Qt::FindDirectChildrenOnly);
    if (tip == nullptr || !tip->isVisible()) {
      return QString("missing");
    }
    const QRect vicinity(expected - QPoint(50, 50), QSize(100, 100));
    if (!vicinity.intersects(tip->geometry())) {
      return QString("misplaced:%1 at %2,%3; expected near %4,%5")
          .arg(tip->text())
          .arg(tip->x())
          .arg(tip->y())
          .arg(expected.x())
          .arg(expected.y());
    }
    return tip->text();
  };

  QCOMPARE(tooltipAt(content.topLeft() + QPoint(12, 12)),
           QString("In Foundry"));
  QCOMPARE(tooltipAt(QPoint(content.right() - 11, content.top() + 12)),
           QString("Vaulted"));
  QCOMPARE(tooltipAt(QPoint(image.left() + 15, image.bottom() - 12)),
           QString("Mastery Rank 10"));
  QCOMPARE(tooltipAt(QPoint(image.right() - 14, image.bottom() - 14)),
           QString("Subsumed"));
  wfgui::hideTooltip();
  QCoreApplication::processEvents();
}

void RelicModelTest::masteryComponentTooltipsUseLocalCoordinates() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items", QJsonArray{QJsonObject{
                    {"id", "test-prime"},
                    {"name", "Test Prime"},
                    {"group", "warframes"},
                    {"owned", false},
                    {"components",
                     QJsonArray{QJsonObject{{"name", "Test Prime Blueprint"},
                                            {"required", 2},
                                            {"owned", 1}}}},
                }}},
  }));

  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Mastery);
  grid.setModel(&model);
  grid.resize(800, 600);
  grid.show();
  QCoreApplication::processEvents();

  const QRect item = grid.visualRect(model.index(0));
  const QRect card = item.adjusted(4, 4, -4, -4);
  const QRect content = card.adjusted(8, 8, -8, -8);
  const int componentAreaLeft = content.right() - 109 + 1;
  const QRect component(componentAreaLeft + (109 - 30) / 2,
                        content.center().y() - 30 / 2, 30, 30);
  const QPoint position = component.center();
  const QPoint expected = grid.viewport()->mapTo(&grid, position);

  wfgui::hideTooltip();
  QCoreApplication::processEvents();
  QHelpEvent event(QEvent::ToolTip, position,
                   grid.viewport()->mapToGlobal(position) +
                       QPoint(10000, 10000));
  QApplication::sendEvent(grid.viewport(), &event);
  QCoreApplication::processEvents();

  QLabel *tip =
      grid.findChild<QLabel *>("wfguiTooltip", Qt::FindDirectChildrenOnly);
  QVERIFY(tip != nullptr);
  QVERIFY(tip->isVisible());
  QVERIFY(!tip->isWindow());
  QCOMPARE(tip->text(), QString("Test Prime Blueprint\nOwned: 1/2"));
  const QRect vicinity(expected - QPoint(50, 50), QSize(100, 100));
  QVERIFY2(vicinity.intersects(tip->geometry()),
           qPrintable(QString("tooltip at %1,%2; expected near %3,%4")
                          .arg(tip->x())
                          .arg(tip->y())
                          .arg(expected.x())
                          .arg(expected.y())));
  wfgui::hideTooltip();
  QCoreApplication::processEvents();
}

void RelicModelTest::componentClicksUseCounterpartyListings() {
  const auto data = [](int owned) {
    return QJsonObject{
        {"items",
         QJsonArray{QJsonObject{
             {"id", "test-prime"},
             {"name", "Test Prime"},
             {"group", "warframe"},
             {"components", QJsonArray{QJsonObject{
                                {"name", "Chassis"},
                                {"market_name", "Test Prime Chassis Blueprint"},
                                {"required", 1},
                                {"owned", owned},
                                {"owned_relic", true},
                            }}},
         }}}};
  };

  PlayerItemModel model;
  QVERIFY(model.replace(data(0)));
  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Foundry);
  QSignalSpy market(&grid, &PlayerItemGridWidget::marketItemRequested);
  QSignalSpy relic(&grid, &PlayerItemGridWidget::relicRewardRequested);
  grid.setModel(&model);
  grid.resize(320, 400);
  grid.show();
  QCoreApplication::processEvents();

  const auto componentRect = [&grid, &model] {
    const QRect card = grid.visualRect(model.index(0)).adjusted(4, 4, -4, -4);
    const QRect content = card.adjusted(8, 8, -8, -8);
    constexpr int size = 39;
    constexpr int gap = 8;
    constexpr int layoutWidth = 3 * size + 2 * gap;
    const int left =
        content.right() - layoutWidth + 1 + (layoutWidth - size) / 2;
    const QRect body(content.left(), content.top() + 33, content.width(), 86);
    return QRect(left, body.center().y() - size / 2, size, size);
  };

  QTest::mouseClick(grid.viewport(), Qt::LeftButton, Qt::NoModifier,
                    componentRect().center());
  QCOMPARE(market.count(), 1);
  QCOMPARE(market.takeFirst().at(1).toString(), QString("sell"));

  QVERIFY(model.replace(data(1)));
  QCoreApplication::processEvents();
  QTest::mouseClick(grid.viewport(), Qt::LeftButton, Qt::NoModifier,
                    componentRect().center());
  QCOMPARE(market.count(), 1);
  QCOMPARE(market.takeFirst().at(1).toString(), QString("buy"));

  const QRect component = componentRect();
  const QRect marker(component.left() - 2, component.bottom() - 19 + 3, 19, 19);
  QTest::mouseClick(grid.viewport(), Qt::LeftButton, Qt::NoModifier,
                    marker.center());
  QCOMPARE(market.count(), 0);
  QCOMPARE(relic.count(), 1);
  QCOMPARE(relic.takeFirst().at(0).toString(),
           QString("Test Prime Chassis Blueprint"));
}

void RelicModelTest::playerItemContextMenuRoutesAcrossPages() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{QJsonObject{
           {"id", "test-prime"},
           {"name", "Test Prime"},
           {"group", "warframes"},
           {"is_prime", true},
           {"has_recipe", true},
           {"from_relics", true},
           {"components", QJsonArray{QJsonObject{
                              {"name", "Chassis"},
                              {"market_name", "Test Prime Chassis Blueprint"},
                              {"required", 1},
                              {"owned", 0},
                              {"relic_drop", true},
                          }}},
       }}},
  }));

  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Mastery);
  QSignalSpy market(&grid, &PlayerItemGridWidget::marketItemRequested);
  QSignalSpy relic(&grid, &PlayerItemGridWidget::relicRewardRequested);
  QSignalSpy foundry(&grid, &PlayerItemGridWidget::foundryItemRequested);
  grid.setModel(&model);
  grid.resize(800, 600);
  grid.show();
  QCoreApplication::processEvents();

  const QRect item = grid.visualRect(model.index(0));
  const QRect card = item.adjusted(4, 4, -4, -4);
  const QRect content = card.adjusted(8, 8, -8, -8);
  const int componentAreaLeft = content.right() - 109 + 1;
  const QRect component(componentAreaLeft + (109 - 30) / 2,
                        content.center().y() - 30 / 2, 30, 30);
  const QPoint global = grid.viewport()->mapToGlobal(component.center());
  QContextMenuEvent event(QContextMenuEvent::Mouse, component.center(), global);
  QApplication::sendEvent(grid.viewport(), &event);
  QCoreApplication::processEvents();

  auto *menu = grid.findChild<QMenu *>("playerItemContextMenu");
  QVERIFY(menu);
  const auto action = [menu](const QString &text) {
    for (QAction *candidate : menu->actions()) {
      if (candidate->text() == text) {
        return candidate;
      }
    }
    return static_cast<QAction *>(nullptr);
  };
  QVERIFY(action("Show in Relic Planner"));
  QVERIFY(action("Show item in Foundry"));
  QVERIFY(action("View WTS listings"));
  QVERIFY(action("View WTB listings"));

  action("Show in Relic Planner")->trigger();
  QCOMPARE(relic.takeFirst().at(0).toString(),
           QString("Test Prime Chassis Blueprint"));
  action("Show item in Foundry")->trigger();
  QCOMPARE(foundry.takeFirst().at(0).toString(), QString("Test Prime"));
  action("View WTS listings")->trigger();
  QCOMPARE(market.takeFirst().at(1).toString(), QString("sell"));
  menu->close();
  QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);

  const QPoint itemPosition(content.left() + 4, content.center().y());
  const QPoint itemGlobal = grid.viewport()->mapToGlobal(itemPosition);
  QContextMenuEvent itemEvent(QContextMenuEvent::Mouse, itemPosition,
                              itemGlobal);
  QApplication::sendEvent(grid.viewport(), &itemEvent);
  QCoreApplication::processEvents();
  menu = grid.findChild<QMenu *>("playerItemContextMenu");
  QVERIFY(menu);
  QAction *showParts = nullptr;
  for (QAction *candidate : menu->actions()) {
    if (candidate->text() == "Show parts in Relic Planner") {
      showParts = candidate;
      break;
    }
  }
  QVERIFY(showParts);
  showParts->trigger();
  QCOMPARE(relic.takeFirst().at(0).toString(), QString("Test Prime"));
  menu->close();
}

void RelicModelTest::relicContextMenuFiltersRewards() {
  RelicModel model;
  QVERIFY(model.replace(recommendations()));
  RelicFilterModel filter;
  filter.setSourceModel(&model);
  RelicGridWidget grid;
  QSignalSpy rewardFilter(&grid, &RelicGridWidget::rewardFilterRequested);
  QSignalSpy market(&grid, &RelicGridWidget::marketItemRequested);
  QSignalSpy foundry(&grid, &RelicGridWidget::foundryItemRequested);
  grid.setModel(&filter);
  grid.resize(900, 500);
  grid.show();
  QCoreApplication::processEvents();

  QCOMPARE(grid.layoutMode(), QListView::SinglePass);
  QVERIFY(grid.uniformItemSizes());
  QVERIFY(grid.findChildren<QWidget *>("relicCardCanvas").isEmpty());
  const QModelIndex first = filter.index(0, 0);
  QVERIFY(first.isValid());
  const QRect item = grid.visualRect(first);
  QVERIFY(item.isValid());
  const wfgui::RelicCardLayout layout =
      wfgui::RelicCardLayout::calculate(item, 1, 1, 1.0);
  const QPoint position = layout.rewardCells.front().center();
  const QPoint global = grid.viewport()->mapToGlobal(position);
  QContextMenuEvent event(QContextMenuEvent::Mouse, position, global);
  QApplication::sendEvent(grid.viewport(), &event);
  QCoreApplication::processEvents();

  auto *menu = grid.findChild<QMenu *>("relicContextMenu");
  QVERIFY(menu);
  const auto action = [menu](const QString &text) {
    for (QAction *candidate : menu->actions()) {
      if (candidate->text() == text) {
        return candidate;
      }
    }
    return static_cast<QAction *>(nullptr);
  };
  QVERIFY(action("Show all relics containing this reward"));
  QVERIFY(action("Show item in Foundry"));
  action("Show all relics containing this reward")->trigger();
  QCOMPARE(rewardFilter.takeFirst().at(0).toString(),
           QString("Saryn Prime Chassis"));
  action("Show item in Foundry")->trigger();
  QCOMPARE(foundry.takeFirst().at(0).toString(),
           QString("Saryn Prime Chassis"));
  action("View WTB listings")->trigger();
  QCOMPARE(market.takeFirst().at(1).toString(), QString("buy"));
  menu->close();

  QJsonObject formaData = recommendations();
  QJsonArray items = formaData.value("items").toArray();
  QJsonObject relic = items.at(0).toObject();
  relic.insert("rewards", QJsonArray{QJsonObject{{"name", "Forma Blueprint"},
                                                 {"rarity", "Common"}}});
  items[0] = relic;
  formaData.insert("items", items);

  RelicModel formaModel;
  QVERIFY(formaModel.replace(formaData));
  RelicFilterModel formaFilter;
  formaFilter.setSourceModel(&formaModel);
  RelicGridWidget formaGrid;
  formaGrid.setModel(&formaFilter);
  formaGrid.resize(900, 500);
  formaGrid.show();
  QCoreApplication::processEvents();
  const QModelIndex formaIndex = formaFilter.index(0, 0);
  QVERIFY(formaIndex.isValid());
  const QRect formaItem = formaGrid.visualRect(formaIndex);
  QVERIFY(formaItem.isValid());
  const wfgui::RelicCardLayout formaLayout =
      wfgui::RelicCardLayout::calculate(formaItem, 1, 1, 1.0);
  const QPoint formaPosition = formaLayout.rewardCells.front().center();
  QContextMenuEvent formaEvent(
      QContextMenuEvent::Mouse, formaPosition,
      formaGrid.viewport()->mapToGlobal(formaPosition));
  QApplication::sendEvent(formaGrid.viewport(), &formaEvent);
  QCoreApplication::processEvents();
  auto *formaMenu = formaGrid.findChild<QMenu *>("relicContextMenu");
  QVERIFY(formaMenu);
  const QStringList formaActions = [&formaMenu] {
    QStringList labels;
    for (QAction *candidate : formaMenu->actions()) {
      labels.push_back(candidate->text());
    }
    return labels;
  }();
  QVERIFY(formaActions.contains("Show all relics containing this reward"));
  QVERIFY(!formaActions.contains("Show item in Foundry"));
  QVERIFY(!formaActions.contains("View WTS listings"));
  QVERIFY(!formaActions.contains("View WTB listings"));
  formaMenu->close();
}

void RelicModelTest::inventoryGridRequestsVisibleQuotes() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items", QJsonArray{QJsonObject{{"id", "part"},
                                       {"name", "Axi A20 Intact"},
                                       {"market_name", "Axi A20 Relic"},
                                       {"group", "relics"},
                                       {"tradable", true}},
                           QJsonObject{{"id", "resource"},
                                       {"name", "Ferrite"},
                                       {"group", "misc"},
                                       {"tradable", false}}}},
  }));
  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Inventory);
  QSignalSpy quotes(&grid, &PlayerItemGridWidget::quotesNeeded);
  grid.setModel(&model);
  grid.resize(800, 600);
  grid.show();
  QCoreApplication::processEvents();

  QTRY_VERIFY(!quotes.isEmpty());
  const QList<QVariant> initial = quotes.takeLast();
  QCOMPARE(initial.at(0).toStringList(), QStringList{"Axi A20 Relic"});
  QCOMPARE(initial.at(1).toBool(), false);

  quotes.clear();
  grid.refreshVisibleQuotes();
  QCOMPARE(quotes.count(), 1);
  QCOMPARE(quotes.takeFirst().at(1).toBool(), true);
}

void RelicModelTest::masteryGridRequestsComponentQuotes() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items",
       QJsonArray{QJsonObject{
           {"id", "frame"},
           {"name", "Test Prime"},
           {"components",
            QJsonArray{
                QJsonObject{{"name", "Test Prime Chassis"},
                            {"market_name", "Test Prime Chassis Blueprint"},
                            {"required", 1},
                            {"owned", 0},
                            {"tradable", true}},
                QJsonObject{{"name", "Test Prime Blueprint"},
                            {"required", 1},
                            {"owned", 0},
                            {"tradable", false}},
            }},
       }}},
  }));
  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Mastery);
  QSignalSpy quotes(&grid, &PlayerItemGridWidget::quotesNeeded);
  grid.setModel(&model);
  grid.resize(800, 600);
  grid.show();
  QCoreApplication::processEvents();

  QTRY_VERIFY(!quotes.isEmpty());
  QCOMPARE(quotes.takeLast().at(0).toStringList(),
           QStringList{"Test Prime Chassis Blueprint"});
}

void RelicModelTest::masteryGridRequestsAllComponentQuotes() {
  QJsonArray items;
  QStringList expected;
  for (int row = 0; row < 24; ++row) {
    const QString marketName = QString("Prime Part %1 Blueprint").arg(row);
    expected.append(marketName);
    items.append(QJsonObject{
        {"id", QString("item-%1").arg(row)},
        {"name", QString("Prime Item %1").arg(row)},
        {"components",
         QJsonArray{QJsonObject{
             {"market_name", marketName}, {"required", 1}, {"owned", 0}}}},
    });
  }
  PlayerItemModel model;
  QVERIFY(model.replace({{"items", items}}));
  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Mastery);
  QSignalSpy quotes(&grid, &PlayerItemGridWidget::quotesNeeded);
  grid.setModel(&model);
  grid.resize(400, 100);
  grid.show();
  QCoreApplication::processEvents();
  quotes.clear();

  grid.requestAllQuotes();

  QCOMPARE(quotes.count(), 1);
  QCOMPARE(quotes.takeFirst().at(0).toStringList(), expected);
}

void RelicModelTest::malformedReplyReconnectsWithoutReplayingMutations() {
  QTemporaryDir directory;
  QVERIFY(directory.isValid());
  QLocalServer server;
  QVERIFY(server.listen(directory.filePath("wfdaemon.sock")));
  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", server.fullServerName().toUtf8());
  const auto restoreSocket = qScopeGuard([oldSocket] {
    if (oldSocket.isNull())
      qunsetenv("RETINUE_DAEMON_SOCKET");
    else
      qputenv("RETINUE_DAEMON_SOCKET", oldSocket);
  });

  DaemonClient client;
  QSignalSpy activity(&client, &DaemonClient::activityReady);
  QSignalSpy assets(&client, &DaemonClient::assetRefreshed);
  QSignalSpy mutations(&client, &DaemonClient::buildSourceFailed);
  client.start();
  QTRY_VERIFY(server.hasPendingConnections());
  QLocalSocket *peer = server.nextPendingConnection();
  QTRY_VERIFY(peer->canReadLine());
  QCOMPARE(
      QJsonDocument::fromJson(peer->readLine()).object().value("op").toString(),
      QString("hello"));
  const QByteArray helloReply =
      QJsonDocument(localContractReply()).toJson(QJsonDocument::Compact) + '\n';
  peer->write(helloReply);
  QTRY_VERIFY(client.connected());

  QHash<QString, QJsonObject> requests;
  const auto received = [&](const QString &operation) {
    while (peer->canReadLine()) {
      const QJsonObject request =
          QJsonDocument::fromJson(peer->readLine()).object();
      requests.insert(request.value("op").toString(), request);
    }
    return requests.contains(operation);
  };
  client.requestActivity();
  client.createBuildGroup({{"name", "Test group"}});
  QTRY_VERIFY(received("activity_view"));
  QTRY_VERIFY(received("build_group_create"));
  const QJsonObject staleEvent{{"event", "asset"},
                               {"data", QJsonObject{{"id", "stale"}}}};
  peer->write("{invalid-json}\n" +
              QJsonDocument(staleEvent).toJson(QJsonDocument::Compact) + '\n');
  QTRY_VERIFY_WITH_TIMEOUT(!client.connected(), 500);
  QCOMPARE(assets.count(), 0);
  QCOMPARE(mutations.count(), 1);
  QVERIFY(mutations.first().at(1).toString().contains("result is unknown"));

  QTRY_VERIFY_WITH_TIMEOUT(server.hasPendingConnections(), 2000);
  peer = server.nextPendingConnection();
  QTRY_VERIFY(peer->canReadLine());
  QCOMPARE(
      QJsonDocument::fromJson(peer->readLine()).object().value("op").toString(),
      QString("hello"));
  requests.clear();
  peer->write(helloReply);
  QTRY_VERIFY(client.connected());
  QTRY_VERIFY(received("activity_view"));
  QTRY_VERIFY(received("build_group_list"));
  QVERIFY(!received("build_group_create"));
  peer->write(
      QJsonDocument(
          QJsonObject{{"id", requests.value("activity_view").value("id")},
                      {"ok", true},
                      {"data", QJsonObject{{"recovered", true}}}})
          .toJson(QJsonDocument::Compact) +
      '\n');
  QTRY_COMPARE(activity.count(), 1);
  QVERIFY(activity.first().at(0).toJsonObject().value("recovered").toBool());
}

void RelicModelTest::subscribesToPlayerUpdatesAndCoalescesViews() {
  QTemporaryDir directory;
  QVERIFY(directory.isValid());
  const QString socketPath = directory.filePath("wfdaemon.sock");
  QLocalServer server;
  QVERIFY(server.listen(socketPath));

  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", socketPath.toUtf8());
  const auto restoreSocket = qScopeGuard([oldSocket] {
    if (oldSocket.isNull()) {
      qunsetenv("RETINUE_DAEMON_SOCKET");
    } else {
      qputenv("RETINUE_DAEMON_SOCKET", oldSocket);
    }
  });

  DaemonClient client;
  QSignalSpy playerChanged(&client, &DaemonClient::playerDatasetChanged);
  QSignalSpy viewReady(&client, &DaemonClient::playerViewReady);
  QSignalSpy assetRefreshed(&client, &DaemonClient::assetRefreshed);
  QSignalSpy buildReady(&client, &DaemonClient::buildSourceReady);
  client.start();
  QTRY_VERIFY(server.hasPendingConnections());
  QLocalSocket *peer = server.nextPendingConnection();
  QVERIFY(peer);
  QTRY_VERIFY(peer->canReadLine());
  const QJsonObject hello = QJsonDocument::fromJson(peer->readLine()).object();
  QCOMPARE(hello.value("op").toString(), QString("hello"));
  QCOMPARE(hello.value("interfaces").toObject().value("diagnostics").toInt(),
           WFCLI_INTERFACE_DIAGNOSTICS);

  peer->write(
      QJsonDocument(localContractReply()).toJson(QJsonDocument::Compact) +
      '\n');
  peer->flush();
  QTRY_VERIFY(client.connected());

  QJsonObject subscription;
  QJsonObject initialDiagnostics;
  QTRY_VERIFY(([&] {
    while (peer->canReadLine()) {
      const QJsonObject request =
          QJsonDocument::fromJson(peer->readLine()).object();
      if (request.value("op").toString() == "subscribe") {
        subscription = request;
      } else if (request.value("op").toString() == "diagnostics_report") {
        initialDiagnostics = request;
      }
    }
    return !subscription.isEmpty() && !initialDiagnostics.isEmpty();
  })());
  QCOMPARE(subscription.value("id").toInteger(), qint64(2));
  QCOMPARE(subscription.value("dataset").toString(), QString("player"));
  QCOMPARE(subscription.value("include_data").toBool(), false);
  QCOMPARE(initialDiagnostics.value("issues").toArray().size(), 0);
  peer->write(QJsonDocument(QJsonObject{
                                {"id", initialDiagnostics.value("id")},
                                {"ok", true},
                            })
                  .toJson(QJsonDocument::Compact) +
              '\n');
  peer->flush();

  client.setResolutionIssue("asset_decode", "wfcd:test.png", "invalid image",
                            "test.png");
  QJsonObject diagnostics;
  QTRY_VERIFY(([&] {
    while (peer->canReadLine()) {
      const QJsonObject request =
          QJsonDocument::fromJson(peer->readLine()).object();
      if (request.value("op").toString() == "diagnostics_report") {
        diagnostics = request;
      }
    }
    return !diagnostics.isEmpty();
  })());
  const QJsonArray issues = diagnostics.value("issues").toArray();
  QCOMPARE(issues.size(), 1);
  QCOMPARE(issues.first().toObject().value("identity").toString(),
           QString("wfcd:test.png"));
  peer->write(
      QJsonDocument(QJsonObject{{"id", diagnostics.value("id")}, {"ok", true}})
          .toJson(QJsonDocument::Compact) +
      '\n');
  peer->flush();

  peer->write(QJsonDocument(QJsonObject{
                                {"id", 2},
                                {"ok", true},
                                {"dataset", "player"},
                                {"data", QJsonObject{{"revision", 4}}},
                            })
                  .toJson(QJsonDocument::Compact) +
              '\n');
  peer->flush();
  QTRY_COMPARE(playerChanged.count(), 1);
  QCOMPARE(playerChanged.at(0).at(0).toLongLong(), qint64(4));
  QCOMPARE(playerChanged.at(0).at(1).toString(), QString());

  const auto sendPlayerEvent = [peer](qint64 revision, const QString &source) {
    peer->write(QJsonDocument(QJsonObject{
                                  {"event", "dataset"},
                                  {"subscription", 2},
                                  {"dataset", "player"},
                                  {"source", source},
                                  {"data", QJsonObject{{"revision", revision}}},
                              })
                    .toJson(QJsonDocument::Compact) +
                '\n');
    peer->flush();
  };
  sendPlayerEvent(5, "collector");
  QTRY_COMPARE(playerChanged.count(), 2);
  QCOMPARE(playerChanged.at(1).at(1).toString(), QString("collector"));
  sendPlayerEvent(5, "inventory");
  QTest::qWait(20);
  QCOMPARE(playerChanged.count(), 2);
  sendPlayerEvent(6, "inventory");
  QTRY_COMPARE(playerChanged.count(), 3);

  peer->write(QJsonDocument(QJsonObject{
                                {"event", "asset"},
                                {"data", QJsonObject{{"ok", true},
                                                     {"source", "wfcd"},
                                                     {"image_name", "item.png"},
                                                     {"path", "/cache/new.png"},
                                                     {"digest", "new"}}},
                            })
                  .toJson(QJsonDocument::Compact) +
              '\n');
  peer->flush();
  QTRY_COMPARE(assetRefreshed.count(), 1);
  QCOMPARE(assetRefreshed.takeFirst().at(0).toJsonObject().value("digest"),
           QJsonValue("new"));

  client.requestPlayerView("inventory");
  QJsonObject firstRequest;
  QTRY_VERIFY(([&] {
    while (peer->canReadLine()) {
      const QJsonObject request =
          QJsonDocument::fromJson(peer->readLine()).object();
      if (request.value("op").toString() == "inventory_view") {
        firstRequest = request;
      }
    }
    return !firstRequest.isEmpty();
  })());
  client.requestPlayerView("inventory");
  peer->write(QJsonDocument(QJsonObject{
                                {"id", firstRequest.value("id")},
                                {"ok", true},
                                {"data", QJsonObject{{"revision", 6}}},
                            })
                  .toJson(QJsonDocument::Compact) +
              '\n');
  peer->flush();
  QTRY_COMPARE(viewReady.count(), 1);

  QJsonObject secondRequest;
  QTRY_VERIFY(([&] {
    while (peer->canReadLine()) {
      const QJsonObject request =
          QJsonDocument::fromJson(peer->readLine()).object();
      if (request.value("op").toString() == "inventory_view") {
        secondRequest = request;
      }
    }
    return !secondRequest.isEmpty();
  })());
  QVERIFY(secondRequest.value("id") != firstRequest.value("id"));
  peer->write(QJsonDocument(QJsonObject{
                                {"id", secondRequest.value("id")},
                                {"ok", true},
                                {"data", QJsonObject{{"revision", 6}}},
                            })
                  .toJson(QJsonDocument::Compact) +
              '\n');
  peer->flush();
  QTRY_COMPARE(viewReady.count(), 2);

  client.requestPlayerView("build_equipment");
  QJsonObject equipmentRequest;
  QTRY_VERIFY(([&] {
    while (peer->canReadLine()) {
      const QJsonObject request =
          QJsonDocument::fromJson(peer->readLine()).object();
      if (request.value("op").toString() == "build_equipment") {
        equipmentRequest = request;
      }
    }
    return !equipmentRequest.isEmpty();
  })());
  peer->write(QJsonDocument(QJsonObject{
                                {"id", equipmentRequest.value("id")},
                                {"ok", true},
                                {"data", QJsonObject{}},
                            })
                  .toJson(QJsonDocument::Compact) +
              '\n');
  peer->flush();
  QTRY_COMPARE(viewReady.count(), 3);

  client.searchBuildItems("revenant", "warframe", 20);
  client.requestBuildList("/Lotus/RevenantPrime");
  client.requestBuildDetail(374539);
  QHash<QString, QJsonObject> buildRequests;
  QTRY_VERIFY(([&] {
    while (peer->canReadLine()) {
      const QJsonObject request =
          QJsonDocument::fromJson(peer->readLine()).object();
      if (request.value("op").toString().startsWith("build_")) {
        buildRequests.insert(request.value("op").toString(), request);
      }
    }
    return buildRequests.size() == 3;
  })());
  const QHash<QString, QJsonObject> buildReplies{
      {"build_search", QJsonObject{{"items", QJsonArray{}}}},
      {"build_list", QJsonObject{{"builds", QJsonArray{}}}},
      {"build_detail",
       QJsonObject{{"identity", QJsonObject{{"external_id", 374539}}},
                   {"fingerprint", "revision"},
                   {"content", QJsonObject{}}}},
  };
  for (auto it = buildRequests.cbegin(); it != buildRequests.cend(); ++it) {
    peer->write(
        QJsonDocument(QJsonObject{{"id", it.value().value("id")},
                                  {"ok", true},
                                  {"data", buildReplies.value(it.key())}})
            .toJson(QJsonDocument::Compact) +
        '\n');
  }
  peer->flush();
  QTRY_COMPARE(buildReady.count(), 3);
  QSet<QString> completedOps;
  for (const QList<QVariant> &arguments : buildReady) {
    completedOps.insert(arguments.at(0).toJsonObject().value("op").toString());
  }
  QCOMPARE(completedOps,
           QSet<QString>({"build_search", "build_list", "build_detail"}));
}

void RelicModelTest::updatesOnlyOlderDaemonContracts() {
  const QJsonObject required{{"datasets", 2}, {"assets", 1}};
  QVERIFY(wfgui::daemonContractNeedsUpdate(
      {{"protocol", 13}, {"compatible", false}}, required, 1));
  QVERIFY(wfgui::daemonContractNeedsUpdate(
      {{"envelope", 1},
       {"interfaces", QJsonObject{{"datasets", 1}, {"assets", 1}}}},
      required, 1));
  QVERIFY(!wfgui::daemonContractNeedsUpdate(
      {{"envelope", 2},
       {"interfaces", QJsonObject{{"datasets", 2}, {"assets", 1}}}},
      required, 1));
  QVERIFY(!wfgui::daemonContractNeedsUpdate(
      {{"envelope", 1},
       {"interfaces", QJsonObject{{"datasets", 3}, {"assets", 1}}}},
      required, 1));
  QVERIFY(!wfgui::daemonContractNeedsUpdate(
      {{"envelope", "invalid"}, {"interfaces", QJsonObject{}}}, required, 1));
}

void RelicModelTest::buildGroupsFollowPlayerRevisions() {
  QTemporaryDir directory;
  QLocalServer server;
  QVERIFY(server.listen(directory.filePath("wfdaemon.sock")));
  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", server.fullServerName().toUtf8());
  const auto restoreSocket = qScopeGuard([oldSocket] {
    if (oldSocket.isNull()) {
      qunsetenv("RETINUE_DAEMON_SOCKET");
    } else {
      qputenv("RETINUE_DAEMON_SOCKET", oldSocket);
    }
  });
  AppController controller;
  auto *client = controller.findChild<DaemonClient *>();
  QVERIFY(client);
  client->playerDatasetChanged(10, "inventory");
  const QJsonObject group{{"id", "group"}, {"name", "Group"},
                         {"definition_id", "/item"}, {"revision", 1},
                         {"player_revision", 10}};
  client->buildGroupEvent("created", group);
  QVERIFY(!controller.buildGroupsLoaded());
  const QJsonObject request{{"op", "build_group_list"}};
  const QJsonObject reply{{"groups", QJsonArray{group}}, {"player_revision", 10}};
  client->buildSourceReady(request, reply);
  QVERIFY(controller.buildGroupsLoaded());

  controller.setActivePage("build-planner");
  client->playerDatasetChanged(11, "inventory");
  QVERIFY(!controller.buildGroupsLoaded());
  QVERIFY(controller.buildGroupsLoading());
  client->buildSourceReady(request, reply);
  QVERIFY(!controller.buildGroupsLoaded());
  QVERIFY(controller.buildGroupsLoading());
  auto updated = group;
  updated.insert("player_revision", 11);
  client->buildSourceReady(request, {{"groups", QJsonArray{updated}},
                                   {"player_revision", 11}});
  QVERIFY(controller.buildGroupsLoaded());
  QVERIFY(!controller.buildGroupsLoading());
  client->buildGroupEvent("planned", group);
  QCOMPARE(controller.buildGroup("group").value("player_revision").toInteger(), 11);
  QVERIFY(!controller.buildGroupsLoaded());
}

void RelicModelTest::cacheMissDoesNotBecomeMarketMiss() {
  QTemporaryDir directory;
  QVERIFY(directory.isValid());
  const QString socketPath = directory.filePath("wfdaemon.sock");
  QLocalServer server;
  QVERIFY(server.listen(socketPath));

  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", socketPath.toUtf8());
  {
    DaemonClient client;
    QSignalSpy resolved(&client, &DaemonClient::marketQuotesResolved);
    QSignalSpy cacheSettled(&client, &DaemonClient::marketQuoteCacheSettled);
    QSignalSpy fetchSettled(&client, &DaemonClient::marketQuoteFetchSettled);
    QSignalSpy variantResolved(&client, &DaemonClient::marketVariantQuoteReady);
    QSignalSpy matches(&client, &DaemonClient::marketMatchesResolved);
    QSignalSpy described(&client, &DaemonClient::marketItemsDescribed);
    QSignalSpy account(&client, &DaemonClient::marketAccountReady);
    client.start();
    QTRY_VERIFY(server.hasPendingConnections());
    QLocalSocket *peer = server.nextPendingConnection();
    QVERIFY(peer);
    QTRY_VERIFY(peer->canReadLine());
    const QJsonObject hello =
        QJsonDocument::fromJson(peer->readLine()).object();
    QCOMPARE(hello.value("op").toString(), QString("hello"));

    peer->write(
        QJsonDocument(localContractReply()).toJson(QJsonDocument::Compact) +
        '\n');
    peer->flush();
    QTRY_VERIFY(client.connected());
    while (peer->canReadLine()) {
      peer->readLine();
    }

    const QStringList itemNames{"Test Prime Blueprint",
                                "Other Prime Blueprint"};
    client.requestMarketQuotes(itemNames);
    QJsonObject cacheRequest;
    QList<QJsonObject> networkRequests;
    QTRY_VERIFY(([&] {
      while (peer->canReadLine()) {
        const QJsonObject request =
            QJsonDocument::fromJson(peer->readLine()).object();
        if (request.value("op").toString() != "market_quote") {
          continue;
        }
        if (request.value("cache_only").toBool()) {
          cacheRequest = request;
        } else {
          networkRequests.append(request);
        }
      }
      return !cacheRequest.isEmpty() && networkRequests.size() == 2;
    })());

    peer->write(QJsonDocument(
                    QJsonObject{
                        {"id", cacheRequest.value("id")},
                        {"ok", true},
                        {"data",
                         QJsonObject{
                             {"quotes", QJsonArray{}},
                             {"missing", QJsonArray::fromStringList(itemNames)},
                         }},
                    })
                    .toJson(QJsonDocument::Compact) +
                '\n');
    peer->flush();
    QTest::qWait(20);
    QCOMPARE(resolved.count(), 0);
    QTRY_COMPARE(cacheSettled.count(), 1);
    QCOMPARE(fetchSettled.count(), 0);

    const auto sendQuote = [peer](const QJsonObject &request, int price) {
      const QString item = request.value("items").toArray().first().toString();
      const QJsonObject quote{{"item", item},
                              {"quote", QJsonObject{{"lowest_sell", price}}}};
      peer->write(QJsonDocument(QJsonObject{
                                    {"id", request.value("id")},
                                    {"ok", true},
                                    {"data",
                                     QJsonObject{
                                         {"quotes", QJsonArray{quote}},
                                         {"missing", QJsonArray{}},
                                     }},
                                })
                      .toJson(QJsonDocument::Compact) +
                  '\n');
      peer->flush();
    };
    sendQuote(networkRequests.at(0), 12);
    QTRY_COMPARE(resolved.count(), 1);
    QCOMPARE(fetchSettled.count(), 0);
    QList<QVariant> result = resolved.takeFirst();
    QCOMPARE(result.at(0).toJsonArray().size(), 1);
    QCOMPARE(result.at(1).toJsonArray(), QJsonArray{});
    sendQuote(networkRequests.at(1), 15);
    QTRY_COMPARE(resolved.count(), 1);
    result = resolved.takeFirst();
    QCOMPARE(result.at(0).toJsonArray().size(), 1);
    QCOMPARE(result.at(1).toJsonArray(), QJsonArray{});
    QTRY_COMPARE(fetchSettled.count(), 1);

    const QJsonObject variantFilters{{"rank", 5}, {"subtype", "blueprint"}};
    client.requestMarketVariantQuote("item-1", variantFilters);
    QJsonObject variantRequest;
    QTRY_VERIFY(([&] {
      while (peer->canReadLine()) {
        const QJsonObject request =
            QJsonDocument::fromJson(peer->readLine()).object();
        if (request.value("op").toString() == "market_quote_variant") {
          variantRequest = request;
        }
      }
      return !variantRequest.isEmpty();
    })());
    QCOMPARE(variantRequest.value("filters").toObject(), variantFilters);
    peer->write(
        QJsonDocument(QJsonObject{
                          {"id", variantRequest.value("id")},
                          {"ok", true},
                          {"data",
                           QJsonObject{
                               {"item", "item-1"},
                               {"quote", QJsonObject{{"lowest_sell", 20}}},
                           }},
                      })
            .toJson(QJsonDocument::Compact) +
        '\n');
    peer->flush();
    QTRY_COMPARE(variantResolved.count(), 1);
    QCOMPARE(variantResolved.takeFirst().at(1).toJsonObject(), variantFilters);

    client.requestMarketMatches("saryn", 5);
    QJsonObject resolveRequest;
    QTRY_VERIFY(([&] {
      while (peer->canReadLine()) {
        const QJsonObject request =
            QJsonDocument::fromJson(peer->readLine()).object();
        if (request.value("op").toString() == "market_resolve") {
          resolveRequest = request;
        }
      }
      return !resolveRequest.isEmpty();
    })());
    QCOMPARE(resolveRequest.value("labels").toArray(), QJsonArray{"saryn"});
    QCOMPARE(resolveRequest.value("limit").toInt(), 5);
    const QJsonArray marketMatches{
        QJsonObject{{"name", "Saryn Prime Set"}, {"slug", "saryn_prime_set"}}};
    peer->write(
        QJsonDocument(QJsonObject{
                          {"id", resolveRequest.value("id")},
                          {"ok", true},
                          {"data",
                           QJsonObject{
                               {"resolutions", QJsonArray{QJsonObject{
                                                   {"label", "saryn"},
                                                   {"matches", marketMatches},
                                               }}},
                           }},
                      })
            .toJson(QJsonDocument::Compact) +
        '\n');
    peer->flush();
    QTRY_COMPARE(matches.count(), 1);
    const QList<QVariant> searchResult = matches.takeFirst();
    QCOMPARE(searchResult.at(0).toString(), QString("saryn"));
    QCOMPARE(searchResult.at(1).toJsonArray(), marketMatches);

    client.requestMarketItems({"item-1"});
    client.marketLogin("tenno@example.test", "secret");
    QJsonObject describeRequest;
    QJsonObject loginRequest;
    QTRY_VERIFY(([&] {
      while (peer->canReadLine()) {
        const QJsonObject request =
            QJsonDocument::fromJson(peer->readLine()).object();
        if (request.value("op").toString() == "market_describe") {
          describeRequest = request;
        } else if (request.value("op").toString() == "market_login") {
          loginRequest = request;
        }
      }
      return !describeRequest.isEmpty() && !loginRequest.isEmpty();
    })());
    QCOMPARE(describeRequest.value("items").toArray(), QJsonArray{"item-1"});
    QCOMPARE(loginRequest.value("email").toString(),
             QString("tenno@example.test"));
    QCOMPARE(loginRequest.value("password").toString(), QString("secret"));

    peer->write(QJsonDocument(QJsonObject{
                                  {"id", describeRequest.value("id")},
                                  {"ok", true},
                                  {"data",
                                   QJsonObject{
                                       {"items", QJsonArray{QJsonObject{
                                                     {"id", "item-1"},
                                                     {"name", "Test Prime"},
                                                 }}},
                                       {"missing", QJsonArray{}},
                                   }},
                              })
                    .toJson(QJsonDocument::Compact) +
                '\n');
    peer->write(QJsonDocument(QJsonObject{
                                  {"id", loginRequest.value("id")},
                                  {"ok", true},
                                  {"data",
                                   QJsonObject{
                                       {"authenticated", true},
                                       {"profile",
                                        QJsonObject{
                                            {"ingameName", "Tenno"},
                                        }},
                                       {"orders", QJsonArray{}},
                                   }},
                              })
                    .toJson(QJsonDocument::Compact) +
                '\n');
    peer->flush();
    QTRY_COMPARE(described.count(), 1);
    QTRY_COMPARE(account.count(), 1);
    QCOMPARE(account.takeFirst().at(0).toString(), QString("login"));

    const QJsonObject createdOrder{{"itemId", "item-1"},
                                   {"type", "sell"},
                                   {"platinum", 12},
                                   {"quantity", 1},
                                   {"visible", false}};
    client.marketCreateOrder(createdOrder);
    client.marketUpdateOrder("order-1", {{"visible", true}});
    client.marketDeleteOrder("order-2");
    client.marketCloseOrder("order-3", 2);
    client.setMarketOrdersVisible(false, "sell");
    client.setMarketPresenceMode("online");
    client.marketLogout();
    QHash<QString, QJsonObject> accountRequests;
    QTRY_VERIFY(([&] {
      while (peer->canReadLine()) {
        const QJsonObject request =
            QJsonDocument::fromJson(peer->readLine()).object();
        const QString operation = request.value("op").toString();
        if (operation.startsWith("market_")) {
          accountRequests.insert(operation, request);
        }
      }
      return accountRequests.size() == 7;
    })());
    QSet<qint64> requestIds;
    for (const QJsonObject &request : std::as_const(accountRequests)) {
      QVERIFY(request.value("id").isDouble());
      requestIds.insert(request.value("id").toInteger());
    }
    QCOMPARE(requestIds.size(), accountRequests.size());
    QCOMPARE(
        accountRequests.value("market_order_create").value("order").toObject(),
        createdOrder);
    const auto verifyMutation = [&accountRequests](const QString &operation,
                                                   const QString &orderId) {
      const QJsonObject request = accountRequests.value(operation);
      QCOMPARE(request.value("order_id").toString(), orderId);
    };
    verifyMutation("market_order_update", "order-1");
    QCOMPARE(
        accountRequests.value("market_order_update").value("patch").toObject(),
        QJsonObject({{"visible", true}}));
    verifyMutation("market_order_delete", "order-2");
    verifyMutation("market_order_close", "order-3");
    QCOMPARE(
        accountRequests.value("market_order_close").value("quantity").toInt(),
        2);
    QCOMPARE(accountRequests.value("market_orders_visibility")
                 .value("visible")
                 .toBool(),
             false);
    QCOMPARE(accountRequests.value("market_orders_visibility")
                 .value("type")
                 .toString(),
             QString("sell"));
    QCOMPARE(
        accountRequests.value("market_presence_set").value("mode").toString(),
        QString("online"));
  }
  if (oldSocket.isNull()) {
    qunsetenv("RETINUE_DAEMON_SOCKET");
  } else {
    qputenv("RETINUE_DAEMON_SOCKET", oldSocket);
  }
}

void RelicModelTest::playerGridPreservesScrollAcrossResort() {
  QJsonArray items;
  for (int row = 0; row < 40; ++row) {
    const QString name = QString("Prime Item %1").arg(row, 2, 10, QChar('0'));
    items.append(QJsonObject{
        {"id", QString("item-%1").arg(row)},
        {"name", name},
        {"mastered", false},
        {"buyable", true},
        {"potential_xp", 3000},
        {"components",
         QJsonArray{QJsonObject{{"name", "Blueprint"},
                                {"market_name", name + " Blueprint"},
                                {"market_required", true},
                                {"required", 1},
                                {"owned", 0}}}},
    });
  }
  PlayerItemModel model;
  QVERIFY(model.replace({{"items", items}}));
  PlayerItemFilterModel filter;
  filter.setSourceModel(&model);
  filter.setPricesLoading(true);
  filter.setMode("platinum");

  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Mastery);
  QCOMPARE(grid.layoutMode(), QListView::SinglePass);
  grid.setModel(&filter);
  grid.resize(800, 300);
  grid.show();
  QCoreApplication::processEvents();
  QTRY_VERIFY(grid.verticalScrollBar()->maximum() > 0);
  grid.verticalScrollBar()->setValue(grid.verticalScrollBar()->maximum() / 2);
  const auto topVisibleId = [&grid, &filter] {
    QModelIndex top;
    QRect topRect;
    for (int row = 0; row < filter.rowCount(); ++row) {
      const QModelIndex candidate = filter.index(row, 0);
      const QRect rect = grid.visualRect(candidate);
      if (!rect.intersects(grid.viewport()->rect())) {
        continue;
      }
      if (!top.isValid() || rect.top() < topRect.top() ||
          (rect.top() == topRect.top() && rect.left() < topRect.left())) {
        top = candidate;
        topRect = rect;
      }
    }
    return top.data(PlayerItemModel::IdRole).toString();
  };
  const QString before = topVisibleId();
  QVERIFY(!before.isEmpty());
  const auto rowForId = [&filter](const QString &id) {
    for (int row = 0; row < filter.rowCount(); ++row) {
      if (filter.index(row, 0).data(PlayerItemModel::IdRole).toString() == id) {
        return row;
      }
    }
    return -1;
  };
  const int beforeOffset =
      grid.visualRect(filter.index(rowForId(before), 0)).top();

  QJsonArray quotes;
  for (int row = 0; row < 40; ++row) {
    quotes.append(QJsonObject{
        {"item",
         QString("Prime Item %1 Blueprint").arg(row, 2, 10, QChar('0'))},
        {"quote", QJsonObject{{"lowest_sell", 40 - row}}},
    });
  }
  model.applyMarketQuotes(quotes, {});
  filter.setPricesLoading(false);
  QTRY_VERIFY(rowForId(before) >= 0);
  QTRY_COMPARE(grid.visualRect(filter.index(rowForId(before), 0)).top(),
               beforeOffset);
  QVERIFY(grid.verticalScrollBar()->value() > 0);
}

void RelicModelTest::busyProgressAnimates() {
  AnimatedProgressBar regular;
  regular.resize(200, 7);
  regular.setRange(0, 0);
  regular.show();

  AnimatedProgressBar thin;
  thin.setObjectName("priceProgress");
  thin.resize(200, 2);
  thin.setRange(0, 0);
  thin.show();
  QCoreApplication::processEvents();

  const QImage firstRegular = regular.grab().toImage();
  const QImage firstThin = thin.grab().toImage();
  QTest::qWait(180);
  QVERIFY(firstRegular != regular.grab().toImage());
  QVERIFY(firstThin != thin.grab().toImage());
}

void RelicModelTest::ownershipFilterKeepsVisibleRowsStable() {
  QJsonObject data = recommendations();
  QJsonArray items = data.value("items").toArray();
  QJsonObject unowned = items.at(1).toObject();
  unowned.insert("amount_owned", 0);
  items.replace(1, unowned);
  data.insert("items", items);

  RelicModel model;
  QVERIFY(model.replace(data));
  RelicFilterModel filter;
  filter.setSourceModel(&model);
  RelicGridWidget grid;
  grid.setModel(&filter);
  grid.resize(900, 500);
  grid.show();
  QCoreApplication::processEvents();

  QCOMPARE(grid.layoutMode(), QListView::SinglePass);
  QVERIFY(grid.uniformItemSizes());
  QVERIFY(grid.findChildren<QWidget *>("relicCardCanvas").isEmpty());
  const QRect initial = grid.visualRect(filter.index(0, 0));
  QVERIFY(initial.isValid());

  filter.setOnlyOwned(false);
  QTRY_COMPARE(filter.rowCount(), 2);
  QTRY_COMPARE(grid.visualRect(filter.index(0, 0)), initial);
  QString error;
  const QPixmap card =
      wfgui::grabCaptureTarget("relic-planner.grid.item", 0, &error);
  QVERIFY2(!card.isNull(), qPrintable(error));
}

void RelicModelTest::compactSearchExpandsOnClick() {
  QWidget host;
  auto *layout = new QHBoxLayout(&host);
  layout->addStretch();
  CompactSearch search("Search items");
  layout->addWidget(&search);
  host.resize(640, 60);
  host.show();
  QCoreApplication::processEvents();
  auto *button = search.findChild<QToolButton *>("compactTool");
  QVERIFY(button);
  QVERIFY(search.editor()->isHidden());
  QCOMPARE(search.width(), 40);

  QTest::mouseClick(button, Qt::LeftButton);
  QTRY_VERIFY(search.editor()->isVisible());
  QTRY_COMPARE(search.width(), 174);

  QTest::mouseClick(button, Qt::LeftButton);
  QTRY_VERIFY(search.editor()->isHidden());
  QCOMPARE(search.width(), 40);

  search.setText("Saryn Prime Chassis");
  QTRY_VERIFY(search.editor()->isVisible());
  QTRY_VERIFY(search.width() > 174);
  QCOMPARE(search.editor()->text(), QString("Saryn Prime Chassis"));
}

void RelicModelTest::normalizesUiScaleInFivePercentSteps() {
  QCOMPARE(wfgui::normalizedUiScalePercent(25), 25);
  QCOMPARE(wfgui::normalizedUiScalePercent(102), 100);
  QCOMPARE(wfgui::normalizedUiScalePercent(103), 105);
  QCOMPARE(wfgui::normalizedUiScalePercent(127), 125);
  QCOMPARE(wfgui::normalizedUiScalePercent(128), 130);
  QCOMPARE(wfgui::normalizedUiScalePercent(999), 175);
}

void RelicModelTest::thumbnailCacheDecodesJpeg() {
  QTemporaryDir directory;
  QVERIFY(directory.isValid());
  const QString path = directory.filePath("mod-art.jpg");
  QImage source(80, 40, QImage::Format_RGB32);
  source.fill(Qt::red);
  QVERIFY(source.save(path, "JPEG"));

  QImage target(64, 64, QImage::Format_ARGB32_Premultiplied);
  QPainter painter(&target);
  const QPixmap thumbnail =
      wfgui::cachedThumbnail(painter, path, QSize(20, 20));
  QCOMPARE(thumbnail.deviceIndependentSize(), QSizeF(20, 10));
}

void RelicModelTest::reportsThumbnailDecodeFailures() {
  wfgui::AssetRef reported;
  QString error;
  bool resolved = true;
  wfgui::setImageIssueReporter(
      [&](const wfgui::AssetRef &asset, const QString &reason, bool fixed) {
        reported = asset;
        error = reason;
        resolved = fixed;
      });
  const auto reset = qScopeGuard([] { wfgui::setImageIssueReporter({}); });

  QImage canvas(32, 32, QImage::Format_ARGB32_Premultiplied);
  QPainter painter(&canvas);
  const wfgui::AssetRef missing =
      wfgui::AssetRef::embedded("missing:test", ":/missing-test.png");
  QVERIFY(wfgui::cachedThumbnail(painter, missing, QSize(24, 24)).isNull());
  QCOMPARE(reported.id, QString("missing:test"));
  QVERIFY(!error.isEmpty());
  QCOMPARE(resolved, false);
}

void RelicModelTest::thumbnailCacheRespectsSizeAndDpr() {
  QTemporaryDir directory;
  QVERIFY(directory.isValid());
  const QString path = directory.filePath("asset.png");
  QImage source(80, 40, QImage::Format_ARGB32_Premultiplied);
  source.fill(Qt::red);
  QVERIFY(source.save(path));

  QImage target(64, 64, QImage::Format_ARGB32_Premultiplied);
  QPainter painter(&target);
  const QPixmap first = wfgui::cachedThumbnail(painter, path, QSize(20, 20));
  const QPixmap cached = wfgui::cachedThumbnail(painter, path, QSize(20, 20));
  QCOMPARE(first.size(), QSize(20, 10));
  QCOMPARE(first.deviceIndependentSize(), QSizeF(20, 10));
  QCOMPARE(cached.cacheKey(), first.cacheKey());

  QImage highDpiTarget(128, 128, QImage::Format_ARGB32_Premultiplied);
  highDpiTarget.setDevicePixelRatio(2.0);
  QPainter highDpiPainter(&highDpiTarget);
  const QPixmap highDpi =
      wfgui::cachedThumbnail(highDpiPainter, path, QSize(20, 20));
  QCOMPARE(highDpi.size(), QSize(40, 20));
  QCOMPARE(highDpi.deviceIndependentSize(), QSizeF(20, 10));
  QCOMPARE(highDpi.devicePixelRatio(), 2.0);
  QVERIFY(highDpi.cacheKey() != first.cacheKey());

  QImage fractionalTarget(80, 80, QImage::Format_ARGB32_Premultiplied);
  fractionalTarget.setDevicePixelRatio(1.25);
  QPainter fractionalPainter(&fractionalTarget);
  const QPixmap fractional =
      wfgui::cachedThumbnail(fractionalPainter, path, QSize(20, 20));
  QCOMPARE(fractional.size(), QSize(25, 12));
  QCOMPARE(fractional.devicePixelRatio(), 1.25);
  QVERIFY(fractional.cacheKey() != first.cacheKey());
  QVERIFY(fractional.cacheKey() != highDpi.cacheKey());
}

void RelicModelTest::alignsFractionalDprThumbnailsToDevicePixels() {
  QImage pixels(4, 4, QImage::Format_ARGB32_Premultiplied);
  for (int y = 0; y < pixels.height(); ++y) {
    for (int x = 0; x < pixels.width(); ++x) {
      pixels.setPixelColor(x, y, (x + y) % 2 == 0 ? Qt::red : Qt::blue);
    }
  }
  QPixmap thumbnail = QPixmap::fromImage(pixels);
  thumbnail.setDevicePixelRatio(1.25);

  QImage target(12, 12, QImage::Format_ARGB32_Premultiplied);
  target.setDevicePixelRatio(1.25);
  target.fill(Qt::transparent);
  QPainter painter(&target);
  painter.setRenderHint(QPainter::SmoothPixmapTransform);
  wfgui::drawContained(painter, QRectF(1, 1, 4, 4), thumbnail);
  painter.end();

  int painted = 0;
  for (int y = 0; y < target.height(); ++y) {
    for (int x = 0; x < target.width(); ++x) {
      const QColor color = target.pixelColor(x, y);
      if (color.alpha() == 0) {
        continue;
      }
      ++painted;
      QVERIFY(color == QColor(Qt::red) || color == QColor(Qt::blue));
    }
  }
  QCOMPARE(painted, 16);
}

void RelicModelTest::widgetThumbnailDecodeCompletesOffPaintPath() {
  QTemporaryDir directory;
  QVERIFY(directory.isValid());
  const QString path = directory.filePath("async-asset.png");
  QImage source(80, 40, QImage::Format_ARGB32_Premultiplied);
  source.fill(Qt::green);
  QVERIFY(source.save(path));

  const QRect dirtyRegion(4, 5, 18, 17);
  ThumbnailProbe probe(path, dirtyRegion);
  probe.resize(32, 32);
  probe.show();
  QTRY_VERIFY_WITH_TIMEOUT(!probe.thumbnail().isNull(), 2000);
  QCOMPARE(probe.thumbnail().deviceIndependentSize(), QSizeF(20, 10));
  QCOMPARE(probe.resolvedPaintRegion(), dirtyRegion);
}

void RelicModelTest::thumbnailWorkIsBoundedWhenGuiFallsBehind() {
  class Burst final : public QWidget {
  public:
    QList<wfgui::AssetRef> assets;
    int loaded = 0;
    void paintEvent(QPaintEvent *) override {
      QPainter painter(this);
      loaded = 0;
      for (const auto &asset : assets) {
        loaded +=
            !wfgui::cachedThumbnail(painter, asset, QSize(512, 512)).isNull();
      }
    }
  } probe;
  QTemporaryDir directory;
  const QString path = directory.filePath("source.png");
  QImage source(512, 512, QImage::Format_ARGB32_Premultiplied);
  source.fill(Qt::green);
  QVERIFY(source.save(path));
  wfgui::clearThumbnailMemoryCache();
  for (int i = 0; i < 24; ++i) {
    auto asset = wfgui::AssetRef::embedded(QString::number(i), path);
    asset.digest = directory.path() + QString::number(i);
    probe.assets.append(asset);
  }
#ifdef Q_OS_LINUX
  const int guiNice = getpriority(PRIO_PROCESS, 0);
#endif
  probe.resize(80, 80);
  probe.show();
  probe.grab();
  const auto submitted = wfgui::thumbnailWorkStats();
  QVERIFY(submitted.active > 0 && submitted.active <= 3);
  QVERIFY(submitted.queued > 0);
  QTest::qSleep(80);
  const auto blocked = wfgui::thumbnailWorkStats();
  QVERIFY(blocked.ready <= 3);
  QVERIFY(blocked.active <= 3);
  QVERIFY(blocked.readyBytes <= 32 * 1024 * 1024);
  QVERIFY(blocked.writeBytes <= 32 * 1024 * 1024);
  QCOMPARE(blocked.queued, submitted.queued);
#ifdef Q_OS_LINUX
  QVERIFY(blocked.workerNice >= 5);
  QCOMPARE(getpriority(PRIO_PROCESS, 0), guiNice);
#endif
  QElapsedTimer elapsed;
  elapsed.start();
  QTimer heartbeat;
  heartbeat.setInterval(1);
  qint64 lastBeat = 0;
  qint64 longestBeat = 0;
  connect(&heartbeat, &QTimer::timeout, &probe, [&] {
    const qint64 now = elapsed.elapsed();
    longestBeat = qMax(longestBeat, now - lastBeat);
    lastBeat = now;
  });
  heartbeat.start();
  QTRY_COMPARE_WITH_TIMEOUT(probe.loaded, probe.assets.size(), 5000);
  QTRY_COMPARE(wfgui::thumbnailWorkStats().active, 0);
  QCOMPARE(wfgui::thumbnailWorkStats().queued, 0);
  qInfo("thumbnail burst: ready=%d ready_bytes=%lld worker_nice=%d "
        "completion_ms=%lld longest_event_gap_ms=%lld",
        blocked.ready, static_cast<long long>(blocked.readyBytes),
        blocked.workerNice, static_cast<long long>(elapsed.elapsed()),
        static_cast<long long>(longestBeat));
}

void RelicModelTest::thumbnailWidgetKeepsTintAndSurfaceAtDpr() {
  QTemporaryDir directory;
  const QString path = directory.filePath("glyph.png");
  QImage source(80, 80, QImage::Format_ARGB32_Premultiplied);
  source.fill(Qt::white);
  QVERIFY(source.save(path));
  ThumbnailWidget widget;
  widget.setAttribute(Qt::WA_TranslucentBackground);
  widget.setObjectName("cycleIcon");
  widget.setStyleSheet(
      "QWidget#cycleIcon { background: #20283e; border-radius: 20px; }");
  widget.setFixedSize(40, 40);
  widget.setImageBounds({20, 20});
  widget.setAsset(wfgui::AssetRef::embedded("glyph", path));
  widget.setTint(Qt::blue);
  const auto colorAt = [&widget](int x, int y) {
    const QPixmap capture = widget.grab();
    const qreal dpr = capture.devicePixelRatio();
    return capture.toImage().pixelColor(qRound(x * dpr), qRound(y * dpr));
  };
  QTRY_COMPARE(colorAt(20, 20), QColor(Qt::blue));
  QCOMPARE(colorAt(5, 20), QColor("#20283e"));
  QCOMPARE(colorAt(0, 0).alpha(), 0);
  const QPixmap capture = widget.grab();
  QCOMPARE(capture.size().width(), qRound(40 * widget.devicePixelRatioF()));
}

void RelicModelTest::derivativeCacheTracksUpstreamIdentity() {
  QTemporaryDir directory;
  QVERIFY(directory.isValid());
  wfgui::DerivativeCache cache(directory.filePath("derivatives"));
  const wfgui::AssetRef first{
      .id = "item:first",
      .source = "wfcd",
      .imageName = "shared.png",
      .path = directory.filePath("source.png"),
      .digest = "digest-one",
      .mediaType = "image/png",
      .size = 100,
      .stale = false,
  };
  QImage image(24, 12, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::green);

  cache.registerAsset(first);
  QVERIFY(cache.store(first, QSize(32, 32), image));
  QCOMPARE(cache.stats().files, 1);
  QCOMPARE(cache.load(first, QSize(32, 32)).size(), image.size());

  wfgui::AssetRef replacement = first;
  replacement.id = "item:second";
  replacement.digest = "digest-two";
  cache.registerAsset(replacement);
  QCOMPARE(cache.stats().files, 0);
  QVERIFY(!cache.store(first, QSize(32, 32), image));
  QVERIFY(cache.store(replacement, QSize(32, 32), image));
  QCOMPARE(cache.stats().files, 1);

  // An older decode finishing late may read/write its digest, never register it.
  QVERIFY(cache.load(first, QSize(32, 32)).isNull());
  QVERIFY(!cache.store(first, QSize(64, 64), image));
  QCOMPARE(cache.load(replacement, QSize(32, 32)).convertToFormat(image.format()),
           image);

  QDirIterator files(cache.root(), {"*.png"}, QDir::Files,
                     QDirIterator::Subdirectories);
  QVERIFY(files.hasNext());
  QFile corrupt(files.next());
  QVERIFY(corrupt.open(QIODevice::WriteOnly | QIODevice::Truncate));
  QCOMPARE(corrupt.write("invalid"), 7);
  corrupt.close();
  QVERIFY(cache.load(replacement, QSize(32, 32)).isNull());
  QCOMPARE(cache.stats().files, 0);

  QVERIFY(cache.store(replacement, QSize(32, 32), image));
  QVERIFY(cache.clear());
  QCOMPARE(cache.stats().files, 0);
  QVERIFY(!cache.store(first, QSize(32, 32), image));
  QVERIFY(cache.store(replacement, QSize(32, 32), image));
  wfgui::DerivativeCache reopened(cache.root());
  QCOMPARE(reopened.load(replacement, QSize(32, 32)).convertToFormat(image.format()),
           image);
}

void RelicModelTest::renderingOlderAssetCannotRegressFreshness() {
  QTemporaryDir sources;
  QVERIFY(sources.isValid());
  QImage image(40, 40, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::green);
  const QString firstPath = sources.filePath("old.png");
  const QString secondPath = sources.filePath("new.png");
  QVERIFY(image.save(firstPath));
  image.fill(Qt::blue);
  QVERIFY(image.save(secondPath));
  const wfgui::AssetRef first{.id = "test-asset", .source = "test",
                            .imageName = "revised.png", .path = firstPath,
                            .digest = "old"};
  auto second = first;
  second.path = secondPath;
  second.digest = "new";
  wfgui::acceptThumbnailAsset(first);
  wfgui::acceptThumbnailAsset(second);
  ThumbnailProbe current(second);
  current.show();
  QTRY_VERIFY(!current.thumbnail().isNull());
  const QSize bounds(qCeil(20 * current.devicePixelRatioF()),
                     qCeil(20 * current.devicePixelRatioF()));
  wfgui::DerivativeCache cache;
  QTRY_VERIFY(!cache.load(second, bounds).isNull());

  ThumbnailProbe old(first);
  old.show();
  QTRY_VERIFY(!old.thumbnail().isNull());
  QVERIFY(!cache.load(second, bounds).isNull());
  QVERIFY(cache.load(first, bounds).isNull());
}

void RelicModelTest::settingsExposeIndependentCacheControls() {
  QTemporaryDir directory;
  QVERIFY(directory.isValid());
  QLocalServer server;
  QVERIFY(server.listen(directory.filePath("wfdaemon.sock")));
  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", server.fullServerName().toUtf8());
  const auto restoreSocket = qScopeGuard([oldSocket] {
    if (oldSocket.isNull()) {
      qunsetenv("RETINUE_DAEMON_SOCKET");
    } else {
      qputenv("RETINUE_DAEMON_SOCKET", oldSocket);
    }
  });

  AppController controller;
  QTRY_VERIFY(server.hasPendingConnections());
  QVERIFY(server.nextPendingConnection());
  SettingsWidget settings(&controller);

  QCOMPARE(settings.findChildren<QWidget *>("settingsGroup").size(), 2);
  QVERIFY(settings.findChild<QLabel *>("settingsDaemonStatus"));
  for (const QString &name :
       {QString("clearMemoryCache"), QString("clearDerivativeCache"),
        QString("clearSourceCache")}) {
    auto *button = settings.findChild<QPushButton *>(name);
    QVERIFY(button);
    QVERIFY(button->property("destructive").toBool());
  }
}

void RelicModelTest::pathReportIsStructured() {
  const QJsonDocument document =
      QJsonDocument::fromJson(wfgui::pathReportJson());
  QVERIFY(document.isObject());
  const QJsonObject report = document.object();
  QCOMPARE(report.value("app").toString(), QString("wfgui"));

  QSet<QString> kinds;
  for (const QJsonValue &value : report.value("paths").toArray()) {
    const QJsonObject entry = value.toObject();
    kinds.insert(entry.value("kind").toString());
    QVERIFY(QFileInfo(entry.value("path").toString()).isAbsolute());
  }
  QCOMPARE(kinds, QSet<QString>({"config", "cache", "derivatives", "runtime"}));
}

void RelicModelTest::marketOrderCardUsesReferenceStructure() {
  int visibilityCalls = 0;
  int editCalls = 0;
  int addCalls = 0;
  int soldCalls = 0;
  int removeCalls = 0;
  int listingCalls = 0;
  MarketOrderCard card(QJsonObject{{"visible", true},
                                   {"type", "sell"},
                                   {"quantity", 5},
                                   {"platinum", 20}},
                       QJsonObject{{"name", "Akvasto Prime Blueprint"}},
                       QJsonObject{{"quote", QJsonObject{{"lowest_sell", 18}}}},
                       1,
                       MarketOrderCardActions{
                           .visibility = [&] { ++visibilityCalls; },
                           .edit = [&] { ++editCalls; },
                           .add = [&] { ++addCalls; },
                           .close = [&] { ++soldCalls; },
                           .remove = [&] { ++removeCalls; },
                           .listings = [&] { ++listingCalls; },
                       });
  card.resize(480, card.height());
  card.show();
  QCoreApplication::processEvents();

  QCOMPARE(card.height(), 107);
  QCOMPARE(card.findChild<QWidget *>("marketOrderTop")->height(), 28);
  QCOMPARE(card.findChild<QWidget *>("marketOrderBody")->height(), 79);
  QCOMPARE(card.findChild<QWidget *>("marketOrderArt")->width(), 70);
  QVERIFY(!card.findChild<QCheckBox *>("marketVisibility"));
  auto *warning = card.findChild<QLabel *>("marketWarning");
  QVERIFY(warning);
  QCOMPARE(warning->geometry().right(),
           card.findChild<QWidget *>("marketOrderTop")->rect().right());

  const auto click = [&card](const char *name) {
    auto *button = card.findChild<QToolButton *>(name);
    QVERIFY(button);
    QTest::mouseClick(button, Qt::LeftButton);
  };
  click("marketVisibility");
  click("marketEdit");
  click("marketAdd");
  click("marketSold");
  click("marketDelete");
  click("marketListings");
  QCOMPARE(visibilityCalls, 1);
  QCOMPARE(editCalls, 1);
  QCOMPARE(addCalls, 1);
  QCOMPARE(soldCalls, 1);
  QCOMPARE(removeCalls, 1);
  QCOMPARE(listingCalls, 1);
}

void RelicModelTest::masteryOwnedBadgeKeepsPaintMargin() {
  PlayerItemModel model;
  QVERIFY(model.replace({
      {"items", QJsonArray{QJsonObject{{"id", "owned-item"},
                                       {"name", "Owned Item"},
                                       {"group", "weapons"},
                                       {"owned", true},
                                       {"rank", 30},
                                       {"max_rank", 30},
                                       {"potential_xp", 3000}}}},
  }));

  PlayerItemGridWidget grid(PlayerItemGridWidget::Kind::Mastery);
  grid.setModel(&model);
  grid.resize(800, 400);
  grid.show();
  QCoreApplication::processEvents();

  const QRect item = grid.visualRect(model.index(0));
  const QRect content = QRect(QPoint(), item.size()).adjusted(12, 12, -12, -12);
  QFont stateFont = grid.font();
  stateFont.setPixelSize(14);
  stateFont.setWeight(QFont::Light);
  const int iconSize = 20;
  const int width =
      iconSize + 4 + QFontMetrics(stateFont).horizontalAdvance("Owned");
  const QRect stateArea(content.right() - 109 + 1, content.top(), 109,
                        content.height());
  const QRect iconRect(stateArea.center().x() - width / 2,
                       stateArea.center().y() - iconSize / 2, iconSize,
                       iconSize);
  const QImage card = grid.viewport()->grab(item).toImage();
  const auto green = [&card](int x, int y) {
    const QColor color = card.pixelColor(x, y);
    return color.green() > 100 && color.green() > color.red() + 25 &&
           color.green() > color.blue() + 20;
  };
  for (int x = iconRect.left(); x <= iconRect.right(); ++x) {
    QVERIFY(!green(x, iconRect.top()));
    QVERIFY(!green(x, iconRect.bottom()));
  }
  for (int y = iconRect.top(); y <= iconRect.bottom(); ++y) {
    QVERIFY(!green(iconRect.left(), y));
    QVERIFY(!green(iconRect.right(), y));
  }
}

void RelicModelTest::marketSearchKeyboardNavigation() {
  QTemporaryDir directory;
  QVERIFY(directory.isValid());
  QLocalServer server;
  QVERIFY(server.listen(directory.filePath("wfdaemon.sock")));
  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", server.fullServerName().toUtf8());
  const auto restoreSocket = qScopeGuard([oldSocket] {
    if (oldSocket.isNull()) {
      qunsetenv("RETINUE_DAEMON_SOCKET");
    } else {
      qputenv("RETINUE_DAEMON_SOCKET", oldSocket);
    }
  });

  AppController controller;
  QTRY_VERIFY(server.hasPendingConnections());
  QVERIFY(server.nextPendingConnection());
  MarketRailWidget rail(&controller);
  rail.resize(420, 720);
  rail.show();
  QCoreApplication::processEvents();
  rail.activateWindow();

  auto *search = rail.findChild<QLineEdit *>("marketRailSearch");
  QVERIFY(search);
  search->setFocus();
  QTRY_VERIFY(search->hasFocus());
  QTest::keyClicks(search, "saryn");

  controller.marketSearchReady(
      "saryn",
      QJsonArray{
          QJsonObject{{"name", "Saryn Prime Set"}, {"slug", "saryn_prime_set"}},
          QJsonObject{{"name", "Saryn Prime Systems Blueprint"},
                      {"slug", "saryn_prime_systems_blueprint"}},
      });

  QCompleter *completer = search->completer();
  QVERIFY(completer);
  QAbstractItemView *popup = completer->popup();
  QTRY_COMPARE(popup->model()->rowCount(), 2);

  QTest::keyClick(search, Qt::Key_Down);
  QTRY_VERIFY(popup->isVisible());
  QCOMPARE(search->text(), QString("saryn"));
  QCOMPARE(popup->currentIndex().data().toString(), QString("Saryn Prime Set"));
  QCOMPARE(popup->model()->rowCount(), 2);

  QTest::keyClick(search, Qt::Key_Down);
  QCOMPARE(search->text(), QString("saryn"));
  QCOMPARE(popup->currentIndex().data().toString(),
           QString("Saryn Prime Systems Blueprint"));

  QTest::keyClick(search, Qt::Key_Up);
  QCOMPARE(search->text(), QString("saryn"));
  QCOMPARE(popup->currentIndex().data().toString(), QString("Saryn Prime Set"));

  QTest::keyClick(search, Qt::Key_Down);
  QTest::keyClick(search, Qt::Key_Return);
  QCOMPARE(search->text(), QString("Saryn Prime Systems Blueprint"));
  QVERIFY(!popup->isVisible());

  controller.marketSearchReady(
      "Saryn Prime Systems Blueprint",
      QJsonArray{QJsonObject{{"name", "Saryn Prime Systems Blueprint"},
                             {"slug", "saryn_prime_systems_blueprint"}}});
  QCoreApplication::processEvents();
  QCOMPARE(popup->model()->rowCount(), 0);
  QVERIFY(!popup->isVisible());
}

void RelicModelTest::filtersExpiredFissures() {
  const QJsonArray fissures = {
      QJsonObject{{"id", "expired"}, {"expiry", "2026-08-02T10:00:00Z"}},
      QJsonObject{{"id", "active"}, {"expiry", "2026-08-02T10:02:00Z"}},
      QJsonObject{{"id", "unknown"}},
  };
  const qint64 now = QDateTime::fromString("2026-08-02T10:01:00Z", Qt::ISODate)
                         .toMSecsSinceEpoch();
  const QJsonArray active = wfgui::activeFissures(fissures, now);

  QCOMPARE(active.size(), 2);
  QCOMPARE(active.at(0).toObject().value("id").toString(), QString("active"));
  QCOMPARE(active.at(1).toObject().value("id").toString(), QString("unknown"));
  QCOMPARE(wfgui::relicEraForFissureTier("Requiem"), QString("requiem"));
  QCOMPARE(wfgui::relicEraForFissureTier("Axi"), QString("axi"));
  QCOMPARE(wfgui::relicEraForFissureTier("Omnia"), QString("all"));
}

void RelicModelTest::rightRailReachesBottom() {
  QTemporaryDir directory;
  QVERIFY(directory.isValid());
  QLocalServer server;
  QVERIFY(server.listen(directory.filePath("wfdaemon.sock")));
  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", server.fullServerName().toUtf8());
  const auto restoreSocket = qScopeGuard([oldSocket] {
    if (oldSocket.isNull()) {
      qunsetenv("RETINUE_DAEMON_SOCKET");
    } else {
      qputenv("RETINUE_DAEMON_SOCKET", oldSocket);
    }
  });

  AppController controller;
  QTRY_VERIFY(server.hasPendingConnections());
  QVERIFY(server.nextPendingConnection());
  ActivityRailWidget rail(&controller);
  QString style;
  for (const auto *layer : {"foundation", "activity"}) {
    QFile file(QStringLiteral(WFGUI_SOURCE_DIR "/resources/styles/%1.qss")
                   .arg(layer));
    QVERIFY(file.open(QIODevice::ReadOnly));
    style += QString::fromUtf8(file.readAll());
  }
  rail.setStyleSheet(style);
  rail.resize(400, 760);
  rail.show();

  for (const auto *name : {"fissureSection", "fissureList"}) {
    auto *panel = rail.findChild<QWidget *>(name);
    QVERIFY(panel);
    QTRY_COMPARE(panel->mapTo(&rail, panel->rect().bottomLeft()).y(),
                 rail.rect().bottom());
    const QImage image = panel->grab().toImage();
    QCOMPARE(image.pixelColor(0, image.height() - 1), QColor("#101623"));
    QCOMPARE(image.pixelColor(image.width() - 1, image.height() - 1),
             QColor("#101623"));
  }
  for (const auto *tab : {"timers", "market"}) {
    QVERIFY(rail.setTab(tab));
    const QImage image = rail.grab().toImage();
    QCOMPARE(image.pixelColor(0, image.height() - 1), QColor("#171e30"));
    QCOMPARE(image.pixelColor(image.width() - 1, image.height() - 1),
             QColor("#171e30"));
  }
}

void RelicModelTest::styleLayersAvoidImplicitSurfaces() {
  QFile foundation(
      QStringLiteral(WFGUI_SOURCE_DIR "/resources/styles/foundation.qss"));
  QVERIFY(foundation.open(QIODevice::ReadOnly));
  const QString foundationText = QString::fromUtf8(foundation.readAll());
  const QRegularExpression widgetRule(
      R"((?:^|\n)QWidget\s*\{([^}]*)\})",
      QRegularExpression::DotMatchesEverythingOption);
  const QRegularExpressionMatch widgetMatch = widgetRule.match(foundationText);
  QVERIFY(widgetMatch.hasMatch());
  QVERIFY(!widgetMatch.captured(1).contains("background"));
  QVERIFY(foundationText.contains(".QWidget {\n  background: transparent;"));

  QFile controls(
      QStringLiteral(WFGUI_SOURCE_DIR "/resources/styles/controls.qss"));
  QVERIFY(controls.open(QIODevice::ReadOnly));
  const QString controlsText = QString::fromUtf8(controls.readAll());
  QVERIFY(controlsText.contains(
      "QCheckBox::indicator:checked {\n  background: #5e44af;"));
  QVERIFY(
      controlsText.contains("image: url(:/resources/market/wfgui-check.png);"));

  QVERIFY(controlsText.contains("QSpinBox::up-button,"));
  QVERIFY(controlsText.contains("QSpinBox::down-button"));
  QVERIFY(controlsText.contains("border-left: 1px solid #313a58;"));
}

void RelicModelTest::marketSpinBoxPaintsVisibleArrows() {
  QFile controls(QStringLiteral(WFGUI_SOURCE_DIR "/resources/styles/controls.qss"));
  QVERIFY(controls.open(QIODevice::ReadOnly));

  wfgui::MarketSpinBox spin;
  spin.setStyleSheet(QString::fromUtf8(controls.readAll()));
  spin.resize(104, 32);
  spin.show();
  QCoreApplication::processEvents();

  const QImage image =
      spin.grab().toImage().convertToFormat(QImage::Format_RGB32);
  int brightPixels = 0;
  for (int y = 0; y < image.height(); ++y) {
    for (int x = image.width() - 18; x < image.width(); ++x) {
      const QColor pixel = image.pixelColor(x, y);
      if (pixel.red() > 170 && pixel.green() > 170 && pixel.blue() > 170) {
        ++brightPixels;
      }
    }
  }
  QVERIFY(brightPixels >= 12);
}

void RelicModelTest::capturesNamedUiTargets() {
  QWidget window;
  window.resize(240, 150);
  auto *panel = new QWidget(&window);
  panel->setGeometry(30, 20, 60, 36);
  panel->setStyleSheet("background: #7653df;");
  wfgui::setCaptureTarget(panel, "test.panel");

  auto *list = new QListWidget(&window);
  list->setGeometry(110, 20, 110, 110);
  list->addItems({"First", "Second", "Third", "Fourth", "Fifth", "Sixth"});
  wfgui::setCaptureTarget(list, "test.list");

  auto *cards = new QWidget(&window);
  cards->setGeometry(30, 75, 60, 55);
  wfgui::setCaptureTarget(cards, "test.cards", true);
  auto *card = new QWidget(cards);
  card->setGeometry(5, 5, 50, 24);
  wfgui::setCaptureItem(card);

  window.show();
  QCoreApplication::processEvents();
  list->scrollToItem(list->item(2), QAbstractItemView::PositionAtTop);
  list->verticalScrollBar()->setValue(list->verticalScrollBar()->value() + 5);
  QCoreApplication::processEvents();

  const QStringList names = wfgui::captureTargetNames();
  QVERIFY(names.contains("test.panel"));
  QVERIFY(names.contains("test.list"));
  QVERIFY(names.contains("test.list.item"));
  QVERIFY(names.contains("test.cards.item"));

  QString error;
  const QPixmap panelImage = wfgui::grabCaptureTarget("test.panel", 6, &error);
  QVERIFY2(!panelImage.isNull(), qPrintable(error));
  QCOMPARE(panelImage.deviceIndependentSize().toSize(), QSize(72, 48));

  const QPixmap itemImage =
      wfgui::grabCaptureTarget("test.list.item", 0, &error);
  QVERIFY2(!itemImage.isNull(), qPrintable(error));
  QVERIFY(itemImage.deviceIndependentSize().width() < list->width());
  QCOMPARE(itemImage.deviceIndependentSize().height(),
           list->visualItemRect(list->item(3)).height());

  const QPixmap cardImage =
      wfgui::grabCaptureTarget("test.cards.item", 2, &error);
  QVERIFY2(!cardImage.isNull(), qPrintable(error));
  QCOMPARE(cardImage.deviceIndependentSize().toSize(), QSize(54, 28));

  QVERIFY(wfgui::grabCaptureTarget("test.missing", 0, &error).isNull());
  QCOMPARE(error, QString("unknown capture target 'test.missing'"));
}

QTEST_MAIN(RelicModelTest)

#include "relic_model_test.moc"
