#include <QEnterEvent>
#include <QFile>
#include <QFrame>
#include <QGridLayout>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QLocalServer>
#include <QPainter>
#include <QScopeGuard>
#include <QTemporaryDir>
#include <QtTest>

#include "app_controller.h"
#include "arcane_card_widget.h"
#include "build_topology_widget.h"
#include "daemon_client.h"
#include "mod_card_widget.h"

class BuildTopologyWidgetTest final : public QObject {
  Q_OBJECT

private slots:
  void rendersConfigAndInstanceState();
  void rendersCanonicalSourcePresentation();
  void usesSourcePresentationWithoutPlayerBaseline();
  void rendersModsAndArcanesByCanonicalSlot();
  void rendersVariableSlotCount();
  void rendersFrameVariantsOnOneContentAnchor();
  void rendersFinalPlanPolarities();
  void centersRowsAndUsesDedicatedEmptyShells();
  void usesOverframeCardGeometry();
  void expandsFilledModWithoutRelayout();
  void keepsExpandedModCenteredAtWindowEdge();
  void centersRankPipsWithoutStretchingAcrossTrack();
  void preservesArcaneGlyphAspectAtFractionalScale();
  void preservesModArtworkAtHighDpi();
  void openPreviewRepaintsWhenArtworkArrives();
};

void BuildTopologyWidgetTest::openPreviewRepaintsWhenArtworkArrives() {
  QTemporaryDir directory;
  QLocalServer server;
  QVERIFY(server.listen(directory.filePath("daemon.sock")));
  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", server.fullServerName().toUtf8());
  const auto restoreSocket = qScopeGuard([oldSocket] {
    if (oldSocket.isNull()) qunsetenv("RETINUE_DAEMON_SOCKET");
    else qputenv("RETINUE_DAEMON_SOCKET", oldSocket);
  });
  AppController controller;
  QWidget host;
  host.resize(600, 500);
  auto *mod = new wfgui::ModCardWidget(
      &controller, {{"id", "mod-1"}, {"role", "mod"}},
      {{"rarity", "common"}, {"asset", QJsonObject{{"id", "test:late-art"}}}},
      "none", &host);
  mod->move(200, 200);
  host.show();
  QTest::qWait(20);
  QEnterEvent enter(QPointF(20, 20), QPointF(220, 220), QPointF(220, 220));
  QCoreApplication::sendEvent(mod, &enter);
  auto *preview = host.findChild<QWidget *>("buildModCardPreview");
  QVERIFY(preview);
  QTRY_COMPARE(preview->size(), QSize(220, 308));
  QTest::qWait(30);
  class PaintCounter final : public QObject {
  public:
    int paints = 0;
    bool eventFilter(QObject *, QEvent *event) override {
      paints += event->type() == QEvent::Paint;
      return false;
    }
  } counter;
  preview->installEventFilter(&counter);
  QImage source(384, 384, QImage::Format_RGB32);
  source.fill(Qt::cyan);
  const QString path = directory.filePath("art.png");
  QVERIFY(source.save(path));
  controller.findChild<DaemonClient *>()->assetsResolved(
      {QJsonObject{{"id", "test:late-art"}, {"ok", true}, {"path", path}}});
  QTRY_VERIFY(counter.paints > 0);
}

void BuildTopologyWidgetTest::preservesModArtworkAtHighDpi() {
  QTemporaryDir directory;
  QVERIFY(directory.isValid());
  QLocalServer server;
  QVERIFY(server.listen(directory.filePath("daemon.sock")));
  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", server.fullServerName().toUtf8());
  const auto restoreSocket = qScopeGuard([oldSocket] {
    if (oldSocket.isNull())
      qunsetenv("RETINUE_DAEMON_SOCKET");
    else
      qputenv("RETINUE_DAEMON_SOCKET", oldSocket);
  });
  AppController controller;
  auto *daemon = controller.findChild<DaemonClient *>();
  QVERIFY(daemon);

  QImage source(384, 384, QImage::Format_RGB32);
  source.fill(Qt::red);
  QPainter sourcePainter(&source);
  sourcePainter.fillRect(192, 0, 192, 384, Qt::green);
  sourcePainter.end();
  const QString path = directory.filePath("split.png");
  QVERIFY(source.save(path));
  daemon->assetsResolved(QJsonArray{
      QJsonObject{{"id", "test:mod"}, {"ok", true}, {"path", path}}});
  wfgui::ModCardWidget widget(
      &controller, {{"id", "mod-1"}, {"role", "mod"}},
      {{"rarity", "common"}, {"asset", QJsonObject{{"id", "test:mod"}}}},
      "none");

  const qreal dpr = widget.devicePixelRatioF();
  const auto artwork = [&widget, dpr](int x) {
    return widget.grab().toImage().pixelColor(qRound(x * dpr),
                                              qRound(40 * dpr));
  };
  QTRY_VERIFY_WITH_TIMEOUT(artwork(85).red() > artwork(85).green() + 80, 2000);
  const QColor right = artwork(110);
  QVERIFY2(
      right.green() > right.red() + 80,
      qPrintable(
          QString("Right artwork at DPR %1: %2").arg(dpr).arg(right.name())));
}

void BuildTopologyWidgetTest::rendersConfigAndInstanceState() {
  const QJsonObject topology{
      {"regions",
       QJsonArray{
           QJsonObject{{"id", "mods"},
                       {"label", "Mods"},
                       {"columns", 1},
                       {"slots", QJsonArray{QJsonObject{{"id", "mod-1"},
                                                        {"label", "Mod 1"},
                                                        {"player_index", 0},
                                                        {"build_slot", 1}}}}},
           QJsonObject{
               {"id", "shards"},
               {"label", "Archon shards"},
               {"columns", 1},
               {"slots", QJsonArray{QJsonObject{{"id", "shard-1"},
                                                {"label", "Shard 1"}}}}},
       }}};
  const QJsonObject instance{
      {"topology", topology},
      {"effective_polarities",
       QJsonArray{QJsonObject{{"slot_id", "mod-1"},
                              {"player_index", 0},
                              {"polarity", "madurai"}}}},
      {"shard_slots",
       QJsonArray{QJsonObject{
           {"slot_id", "shard-1"},
           {"upgrade",
            QJsonObject{{"upgrade_type", "/Lotus/Archon/Crimson"}}}}}},
      {"configs", QJsonArray{QJsonObject{
                      {"config_index", 1},
                      {"ability_override", QJsonArray{"/Lotus/Powers/Roar"}},
                      {"upgrade_slots",
                       QJsonArray{QJsonObject{{"slot", 0},
                                              {"topology_slot", "mod-1"},
                                              {"name", "Vitality"},
                                              {"rank", 10},
                                              {"polarity", "vazarin"},
                                              {"slot_polarity", "madurai"},
                                              {"polarity_state", "mismatched"},
                                              {"effective_drain", 15}}}}}}},
  };

  BuildTopologyWidget widget;
  widget.setPlayerInstance(instance, 1);
  const QList<QLabel *> names = widget.findChildren<QLabel *>("buildSlotName");
  QStringList texts;
  for (const QLabel *label : names) {
    texts.append(label->text());
  }
  QVERIFY(texts.contains("Crimson"));
  const QWidget *mod = widget.findChild<QWidget *>("buildModCard");
  QVERIFY(mod);
  QCOMPARE(mod->property("upgradeName").toString(), QString("Vitality"));
  QCOMPARE(mod->property("modPolarity").toString(), QString("vazarin"));
  QCOMPARE(mod->property("slotPolarity").toString(), QString("madurai"));
  QCOMPARE(mod->property("polarityState").toString(), QString("mismatched"));
  QCOMPARE(mod->property("effectiveDrain").toInt(), 15);
  QVERIFY(mod->property("showsExternalPolarity").toBool());
  const QLabel *override = widget.findChild<QLabel *>("buildAbilityOverride");
  QVERIFY(override);
  QCOMPARE(override->text(), QString("Ability override: Roar"));
}

void BuildTopologyWidgetTest::rendersModsAndArcanesByCanonicalSlot() {
  QJsonArray modSlots;
  for (int visual = 0; visual < 8; ++visual) {
    modSlots.append(QJsonObject{{"id", QString("mod-%1").arg(visual + 1)},
                                {"label", QString("Mod %1").arg(visual + 1)},
                                {"role", "mod"},
                                {"player_index", 7 - visual},
                                {"build_slot", 8 - visual}});
  }
  const QJsonObject topology{
      {"regions",
       QJsonArray{QJsonObject{{"id", "mods"},
                              {"label", "Mods"},
                              {"columns", 4},
                              {"slots", modSlots}},
                  QJsonObject{{"id", "arcanes"},
                              {"label", "Arcanes"},
                              {"columns", 1},
                              {"slots",
                               QJsonArray{QJsonObject{{"id", "arcane-1"},
                                                      {"label", "Arcane 1"},
                                                      {"role", "arcane"},
                                                      {"player_index", 9},
                                                      {"build_slot", 10}}}}}}}};
  const QJsonObject instance{
      {"topology", topology},
      {"configs",
       QJsonArray{QJsonObject{
           {"config_index", 0},
           {"upgrade_slots",
            QJsonArray{
                QJsonObject{{"slot", 7},
                            {"topology_slot", "mod-1"},
                            {"name", "Primed Pistol Gambit"},
                            {"rank", 10},
                            {"drain", 12},
                            {"effective_drain", 6},
                            {"mod_variant", "galvanized"},
                            {"polarity", "madurai"},
                            {"slot_polarity", "omni"},
                            {"polarity_state", "matched"},
                            {"asset", QJsonObject{{"id", "/mod/gambit"}}}},
                QJsonObject{{"slot", 0},
                            {"topology_slot", "mod-8"},
                            {"name", "Lethal Torrent"},
                            {"rank", 5}},
                QJsonObject{
                    {"slot", 9},
                    {"topology_slot", "arcane-1"},
                    {"role", "arcane"},
                    {"name", "Secondary Outburst"},
                    {"rank", 5},
                    {"rarity", "legendary"},
                    {"asset", QJsonObject{{"id", "/arcane/outburst"}}}}}}}}}};

  BuildTopologyWidget widget;
  widget.setPlayerInstance(instance, 0);
  const QList<QWidget *> mods = widget.findChildren<QWidget *>("buildModCard");
  QCOMPARE(mods.size(), 8);
  QList<QWidget *> filled;
  for (QWidget *mod : mods) {
    if (!mod->property("empty").toBool()) {
      filled.append(mod);
    }
  }
  QCOMPARE(filled.size(), 2);
  QCOMPARE(filled.at(0)->property("upgradeName").toString(),
           QString("Primed Pistol Gambit"));
  QCOMPARE(filled.at(0)->property("effectiveDrain").toInt(), 6);
  QCOMPARE(filled.at(0)->property("polarityState").toString(),
           QString("matched"));
  QCOMPARE(filled.at(0)->property("slotPolarity").toString(), QString("omni"));
  QCOMPARE(filled.at(0)->property("modVariant").toString(),
           QString("galvanized"));
  QVERIFY(filled.at(0)->property("showsExternalPolarity").toBool());
  QCOMPARE(filled.at(1)->property("upgradeName").toString(),
           QString("Lethal Torrent"));
  const QWidget *arcane = widget.findChild<QWidget *>("buildArcaneCard");
  QVERIFY(arcane);
  QCOMPARE(arcane->property("upgradeName").toString(),
           QString("Secondary Outburst"));
  QCOMPARE(arcane->property("rarity").toString(), QString("legendary"));
  QCOMPARE(arcane->size(), QSize(160, 108));
  QCOMPARE(widget.findChildren<QFrame *>("buildTopologySlot").size(), 0);

  const QString capture = qEnvironmentVariable("WFGUI_MIXED_TOPOLOGY_CAPTURE");
  if (!capture.isEmpty()) {
    widget.resize(900, 430);
    widget.show();
    QTest::qWait(10);
    QVERIFY2(widget.grab().save(capture), qPrintable(capture));
  }
}

void BuildTopologyWidgetTest::preservesArcaneGlyphAspectAtFractionalScale() {
  AppController controller;
  const QJsonObject slot{
      {"id", "arcane-1"}, {"label", "Arcane 1"}, {"role", "arcane"}};
  const QJsonObject upgrade{{"name", "Test Arcane"},
                            {"rarity", "rare"},
                            {"asset", QJsonObject{{"id", "builtin:forma"}}}};
  wfgui::ArcaneCardWidget widget(&controller, slot, upgrade);

  for (const qreal dpr : {1.0, 1.25, 2.0}) {
    QImage image(qRound(widget.width() * dpr), qRound(widget.height() * dpr),
                 QImage::Format_ARGB32_Premultiplied);
    image.setDevicePixelRatio(dpr);
    image.fill(Qt::transparent);
    QPainter painter(&image);
    widget.render(&painter);
    painter.end();

    QRect cyanBounds;
    for (int y = 0; y < image.height(); ++y) {
      for (int x = 0; x < image.width(); ++x) {
        const QColor pixel = image.pixelColor(x, y);
        if (pixel.alpha() > 64 && pixel.green() > pixel.red() + 20 &&
            pixel.blue() > pixel.red() + 20) {
          cyanBounds |= QRect(x, y, 1, 1);
        }
      }
    }
    QVERIFY(cyanBounds.isValid());
    const qreal aspect = static_cast<qreal>(cyanBounds.width()) /
                         static_cast<qreal>(cyanBounds.height());
    QVERIFY2(aspect > 0.75 && aspect < 1.25,
             qPrintable(QString("Arcane glyph distorted to %1:1").arg(aspect)));
  }
}

void BuildTopologyWidgetTest::rendersCanonicalSourcePresentation() {
  QJsonArray topologySlots;
  for (int visual = 0; visual < 8; ++visual) {
    topologySlots.append(
        QJsonObject{{"id", QString("mod-%1").arg(visual + 1)},
                    {"label", QString("Mod %1").arg(visual + 1)},
                    {"role", "mod"},
                    {"player_index", 7 - visual},
                    {"build_slot", 8 - visual}});
  }
  const QJsonObject topology{
      {"regions", QJsonArray{QJsonObject{{"id", "mods"},
                                         {"label", "Mods"},
                                         {"columns", 4},
                                         {"slots", topologySlots}}}}};
  const QJsonObject baseline{{"topology", topology}};
  const QJsonObject revision{
      {"content", QJsonObject{{"slots", QJsonArray{QJsonObject{
                                            {"source_slot", 1},
                                            {"name", "Wrong raw slot"}}}}}},
      {"presentation",
       QJsonObject{
           {"upgrades", QJsonArray{QJsonObject{{"source_slot", 1},
                                               {"topology_slot", "mod-8"},
                                               {"name", "System Reroute"}},
                                   QJsonObject{{"source_slot", 8},
                                               {"topology_slot", "mod-1"},
                                               {"name", "Cold Snap"}}}}}}};

  BuildTopologyWidget widget;
  widget.setSourceRevision(revision, baseline);
  const QList<QWidget *> mods = widget.findChildren<QWidget *>("buildModCard");
  QCOMPARE(mods.size(), 8);
  QCOMPARE(mods.at(0)->property("slotId").toString(), QString("mod-1"));
  QCOMPARE(mods.at(0)->property("upgradeName").toString(),
           QString("Cold Snap"));
  QCOMPARE(mods.at(7)->property("slotId").toString(), QString("mod-8"));
  QCOMPARE(mods.at(7)->property("upgradeName").toString(),
           QString("System Reroute"));
}

void BuildTopologyWidgetTest::usesSourcePresentationWithoutPlayerBaseline() {
  const QJsonObject topology{
      {"regions", QJsonArray{QJsonObject{
                      {"id", "mods"},
                      {"label", "Mods"},
                      {"columns", 1},
                      {"slots", QJsonArray{QJsonObject{{"id", "mod-1"},
                                                       {"label", "Mod 1"},
                                                       {"role", "mod"},
                                                       {"build_slot", 1}}}}}}}};
  const QJsonObject revision{
      {"content", QJsonObject{{"slots", QJsonArray{}}}},
      {"presentation",
       QJsonObject{
           {"topology", topology},
           {"upgrades",
            QJsonArray{QJsonObject{
                {"source_slot", 1},
                {"topology_slot", "mod-1"},
                {"name", "Galvanized Chamber"},
                {"asset", QJsonObject{{"id", "/Lotus/Mods/Test"},
                                      {"source", "wfcd"},
                                      {"image_name", "test.png"}}}}}}}}};

  BuildTopologyWidget widget;
  widget.setSourceRevision(revision, {});
  const QWidget *mod = widget.findChild<QWidget *>("buildModCard");
  QVERIFY(mod);
  QCOMPARE(mod->property("upgradeName").toString(),
           QString("Galvanized Chamber"));
  QVERIFY(!mod->property("empty").toBool());
}

void BuildTopologyWidgetTest::rendersVariableSlotCount() {
  QJsonArray slotIds;
  for (int visual = 0; visual < 12; ++visual) {
    slotIds.append(QJsonObject{{"id", QString("mod-%1").arg(visual + 1)},
                               {"label", QString("Mod %1").arg(visual + 1)},
                               {"role", "mod"},
                               {"player_index", 11 - visual},
                               {"build_slot", 12 - visual}});
  }
  const QJsonObject instance{
      {"topology",
       QJsonObject{{"regions", QJsonArray{QJsonObject{{"id", "mods"},
                                                      {"label", "Mods"},
                                                      {"columns", 4},
                                                      {"slots", slotIds}}}}}},
      {"configs", QJsonArray{QJsonObject{{"config_index", 0}}}}};

  BuildTopologyWidget widget;
  widget.setPlayerInstance(instance, 0);
  const QList<QWidget *> mods = widget.findChildren<QWidget *>("buildModCard");
  QCOMPARE(mods.size(), 12);
  for (const QWidget *mod : mods) {
    QVERIFY(mod->property("empty").toBool());
  }
  QVERIFY(widget.findChildren<QFrame *>("buildTopologySlot").isEmpty());
  QVERIFY(widget.findChildren<QWidget *>("buildArcaneCard").isEmpty());
}

void BuildTopologyWidgetTest::rendersFrameVariantsOnOneContentAnchor() {
  const QStringList variants{"standard", "galvanized", "amalgam", "riven"};
  QJsonArray topologySlots;
  QJsonArray upgrades;
  for (int index = 0; index < variants.size(); ++index) {
    const QString id = QString("mod-%1").arg(index + 1);
    topologySlots.append(
        QJsonObject{{"id", id},
                    {"label", QString("Mod %1").arg(index + 1)},
                    {"role", "mod"},
                    {"player_index", index},
                    {"build_slot", index + 1}});
    upgrades.append(QJsonObject{{"slot", index},
                                {"topology_slot", id},
                                {"name", variants.at(index)},
                                {"rank", 10},
                                {"max_rank", 10},
                                {"effective_drain", 7},
                                {"polarity", "madurai"},
                                {"slot_polarity", "madurai"},
                                {"polarity_state", "matched"},
                                {"rarity", "rare"},
                                {"mod_variant", variants.at(index)}});
  }
  const QJsonObject instance{
      {"topology", QJsonObject{{"regions", QJsonArray{QJsonObject{
                                               {"id", "mods"},
                                               {"label", "Mods"},
                                               {"columns", 4},
                                               {"slots", topologySlots}}}}}},
      {"configs", QJsonArray{QJsonObject{{"config_index", 0},
                                         {"upgrade_slots", upgrades}}}}};

  BuildTopologyWidget widget;
  widget.resize(900, 190);
  widget.setPlayerInstance(instance, 0);
  widget.show();
  QTest::qWait(10);

  const QList<QWidget *> cards = widget.findChildren<QWidget *>("buildModCard");
  QCOMPARE(cards.size(), variants.size());
  for (int index = 0; index < cards.size(); ++index) {
    QCOMPARE(cards.at(index)->property("modVariant").toString(),
             variants.at(index));
    QCOMPARE(cards.at(index)->geometry().top(),
             cards.first()->geometry().top());
    QCOMPARE(cards.at(index)->height(), cards.first()->height());
    QVERIFY(!cards.at(index)->property("showsExternalPolarity").toBool());
  }
  QVERIFY(QFile::exists(":/resources/mod-frames/GalvanizedFrameTop.png"));
  QVERIFY(QFile::exists(":/resources/mod-frames/LegendarySideLight.png"));
  QVERIFY(QFile::exists(":/resources/mod-frames/LegendaryLowerTab.png"));

  const QString capture = qEnvironmentVariable("WFGUI_MOD_CARD_CAPTURE");
  if (!capture.isEmpty()) {
    QVERIFY2(widget.grab().save(capture), qPrintable(capture));
  }
}

void BuildTopologyWidgetTest::usesOverframeCardGeometry() {
  const QJsonObject topology{
      {"regions",
       QJsonArray{
           QJsonObject{
               {"id", "mods"},
               {"label", "Mods"},
               {"columns", 2},
               {"slots",
                QJsonArray{QJsonObject{{"id", "mod-1"}, {"role", "mod"}},
                           QJsonObject{{"id", "mod-2"}, {"role", "mod"}}}}},
           QJsonObject{
               {"id", "arcanes"},
               {"label", "Arcanes"},
               {"columns", 2},
               {"slots",
                QJsonArray{
                    QJsonObject{{"id", "arcane-1"}, {"role", "arcane"}},
                    QJsonObject{{"id", "arcane-2"}, {"role", "arcane"}}}}}}}};
  const QJsonObject instance{
      {"topology", topology},
      {"configs", QJsonArray{QJsonObject{{"config_index", 0}}}}};

  BuildTopologyWidget widget;
  widget.resize(900, 440);
  widget.setPlayerInstance(instance, 0);
  widget.show();
  QTest::qWait(10);

  const QList<QWidget *> mods = widget.findChildren<QWidget *>("buildModCard");
  QCOMPARE(mods.size(), 2);
  for (const QWidget *mod : mods) {
    QCOMPARE(mod->size(), QSize(200, 110));
  }
  QCOMPARE(qAbs(mods.at(1)->geometry().center().x() -
                mods.at(0)->geometry().center().x()),
           204);

  const QList<QWidget *> arcanes =
      widget.findChildren<QWidget *>("buildArcaneCard");
  QCOMPARE(arcanes.size(), 2);
  for (const QWidget *arcane : arcanes) {
    QCOMPARE(arcane->size(), QSize(160, 108));
  }

  const QList<QWidget *> grids =
      widget.findChildren<QWidget *>("buildRegionGrid");
  QCOMPARE(grids.size(), 2);
  for (const QWidget *grid : grids) {
    const auto *layout = qobject_cast<QGridLayout *>(grid->layout());
    QVERIFY(layout);
    const int expected =
        grid->property("regionId").toString() == "arcanes" ? 0 : 4;
    QCOMPARE(layout->horizontalSpacing(), expected);
    QCOMPARE(layout->verticalSpacing(), 0);
  }
}

void BuildTopologyWidgetTest::expandsFilledModWithoutRelayout() {
  const QJsonObject topology{
      {"regions", QJsonArray{QJsonObject{
                      {"id", "mods"},
                      {"label", "Mods"},
                      {"columns", 1},
                      {"slots", QJsonArray{QJsonObject{{"id", "mod-1"},
                                                       {"role", "mod"}}}}}}}};
  const QJsonObject upgrade{
      {"topology_slot", "mod-1"},
      {"name", "Primed Target Cracker"},
      {"rank", 0},
      {"max_rank", 5},
      {"base_drain", 2},
      {"effective_drain", 2},
      {"rarity", "rare"},
      {"compat_name", "Melee"},
      {"effects", QJsonArray{"+187% Critical Chance",
                             "+93% Critical Chance while aiming"}}};
  const QJsonObject instance{
      {"topology", topology},
      {"configs",
       QJsonArray{QJsonObject{{"config_index", 0},
                              {"upgrade_slots", QJsonArray{upgrade}}}}}};

  BuildTopologyWidget widget;
  widget.resize(700, 430);
  widget.setPlayerInstance(instance, 0);
  widget.show();
  QTest::qWait(10);
  QWidget *mod = widget.findChild<QWidget *>("buildModCard");
  QVERIFY(mod);
  const QRect compactGeometry = mod->geometry();

  QEnterEvent enter(mod->rect().center(), mod->rect().center(),
                    mod->mapToGlobal(mod->rect().center()));
  QCoreApplication::sendEvent(mod, &enter);
  QTRY_VERIFY_WITH_TIMEOUT(
      widget.findChild<QWidget *>("buildModCardPreview") != nullptr, 200);
  QWidget *preview = widget.findChild<QWidget *>("buildModCardPreview");
  QTRY_COMPARE_WITH_TIMEOUT(preview->size(), QSize(220, 308), 300);
  QCOMPARE(mod->geometry(), compactGeometry);
  const QPoint modCenter = mod->mapTo(&widget, mod->rect().center());
  QVERIFY(qAbs(preview->geometry().center().x() - modCenter.x()) <= 1);
  const QImage previewImage = preview->grab().toImage();
  QRect visible;
  for (int y = 0; y < previewImage.height(); ++y) {
    for (int x = 0; x < previewImage.width(); ++x) {
      if (previewImage.pixelColor(x, y).alpha() > 8) {
        visible |= QRect(x, y, 1, 1);
      }
    }
  }
  QVERIFY(!visible.isEmpty());
  QVERIFY(qAbs(visible.center().x() - previewImage.rect().center().x()) <= 2);

  const QString capture =
      qEnvironmentVariable("WFGUI_MOD_CARD_EXPANDED_CAPTURE");
  if (!capture.isEmpty()) {
    QVERIFY2(widget.grab().save(capture), qPrintable(capture));
  }

  QEvent leave(QEvent::Leave);
  QCoreApplication::sendEvent(mod, &leave);
  QTRY_VERIFY_WITH_TIMEOUT(
      widget.findChild<QWidget *>("buildModCardPreview") == nullptr, 400);
  QCOMPARE(mod->geometry(), compactGeometry);
}

void BuildTopologyWidgetTest::keepsExpandedModCenteredAtWindowEdge() {
  QWidget host;
  host.resize(300, 430);
  const QJsonObject slot{{"id", "mod-1"}, {"role", "mod"}};
  const QJsonObject upgrade{{"name", "Serration"},
                            {"rank", 10},
                            {"max_rank", 10},
                            {"effects", QJsonArray{"+165% Damage"}}};
  auto *mod = new wfgui::ModCardWidget(nullptr, slot, upgrade, "none", &host);
  mod->move(host.width() - mod->width(), 150);
  host.show();
  QTest::qWait(10);

  QEnterEvent enter(mod->rect().center(), mod->rect().center(),
                    mod->mapToGlobal(mod->rect().center()));
  QCoreApplication::sendEvent(mod, &enter);
  QTRY_VERIFY_WITH_TIMEOUT(
      host.findChild<QWidget *>("buildModCardPreview") != nullptr, 200);
  QWidget *preview = host.findChild<QWidget *>("buildModCardPreview");
  QTRY_COMPARE_WITH_TIMEOUT(preview->size(), QSize(220, 308), 300);
  const QPoint sourceCenter = mod->mapTo(&host, mod->rect().center());
  QVERIFY(qAbs(preview->geometry().center().x() - sourceCenter.x()) <= 1);
}

void BuildTopologyWidgetTest::centersRankPipsWithoutStretchingAcrossTrack() {
  const QJsonObject slot{{"id", "mod-1"}, {"role", "mod"}};
  const QJsonObject upgrade{{"name", "Serration"},
                            {"rank", 10},
                            {"max_rank", 10},
                            {"rarity", "rare"}};
  wfgui::ModCardWidget mod(nullptr, slot, upgrade, "none");
  mod.show();
  QTest::qWait(10);

  const QImage image = mod.grab().toImage();
  const QString capture = qEnvironmentVariable("WFGUI_MOD_CARD_CAPTURE");
  if (!capture.isEmpty()) {
    QVERIFY2(image.save(capture), qPrintable(capture));
  }
  QRect pips;
  const QColor pipColor("#a6e6ff");
  for (int y = image.height() * 4 / 5; y < image.height(); ++y) {
    for (int x = 0; x < image.width(); ++x) {
      if (image.pixelColor(x, y) == pipColor) {
        pips |= QRect(x, y, 1, 1);
      }
    }
  }

  QVERIFY(!pips.isEmpty());
  QVERIFY(pips.width() / image.devicePixelRatio() <= 80.0);
  QVERIFY(qAbs(pips.center().x() - image.rect().center().x()) <=
          2.0 * image.devicePixelRatio());
}

void BuildTopologyWidgetTest::rendersFinalPlanPolarities() {
  const QJsonObject baseline{
      {"topology",
       QJsonObject{
           {"regions",
            QJsonArray{QJsonObject{
                {"id", "mods"},
                {"label", "Mods"},
                {"columns", 1},
                {"slots", QJsonArray{QJsonObject{{"id", "mod-1"},
                                                 {"label", "Mod 1"},
                                                 {"player_index", 0},
                                                 {"build_slot", 1}}}}}}}}}};
  const QJsonObject result{
      {"final_polarities", QJsonArray{QJsonObject{{"slot_id", "mod-1"},
                                                  {"player_index", 0},
                                                  {"polarity", "naramon"}}}}};

  BuildTopologyWidget widget;
  widget.setPlanResult(result, baseline);
  const QWidget *slot = widget.findChild<QWidget *>("buildModCard");
  QVERIFY(slot);
  QCOMPARE(slot->property("slotPolarity").toString(), QString("naramon"));
}

void BuildTopologyWidgetTest::centersRowsAndUsesDedicatedEmptyShells() {
  const QJsonObject topology{
      {"regions",
       QJsonArray{
           QJsonObject{{"id", "special"},
                       {"label", "Special slots"},
                       {"columns", 4},
                       {"slots", QJsonArray{QJsonObject{{"id", "aura"},
                                                        {"label", "Aura"},
                                                        {"role", "aura"}},
                                            QJsonObject{{"id", "exilus"},
                                                        {"label", "Exilus"},
                                                        {"role", "exilus"}}}}},
           QJsonObject{{"id", "mods"},
                       {"label", "Mods"},
                       {"columns", 4},
                       {"slots", QJsonArray{QJsonObject{{"id", "mod-1"},
                                                        {"label", "Mod 1"},
                                                        {"role", "mod"}},
                                            QJsonObject{{"id", "mod-2"},
                                                        {"label", "Mod 2"},
                                                        {"role", "mod"}}}}},
           QJsonObject{
               {"id", "arcanes"},
               {"label", "Arcanes"},
               {"columns", 4},
               {"slots", QJsonArray{QJsonObject{{"id", "arcane-1"},
                                                {"label", "Arcane 1"},
                                                {"role", "arcane"}},
                                    QJsonObject{{"id", "arcane-2"},
                                                {"label", "Arcane 2"},
                                                {"role", "arcane"}}}}}}}};
  const QJsonObject instance{
      {"topology", topology},
      {"configs", QJsonArray{QJsonObject{{"config_index", 0}}}}};

  BuildTopologyWidget widget;
  widget.resize(1100, 520);
  widget.setPlayerInstance(instance, 0);
  widget.show();
  QTest::qWait(10);

  const QList<QWidget *> grids =
      widget.findChildren<QWidget *>("buildRegionGrid");
  QCOMPARE(grids.size(), 3);
  for (const QWidget *grid : grids) {
    const int center = grid->mapTo(&widget, QPoint{}).x() + grid->width() / 2;
    QVERIFY(qAbs(center - widget.width() / 2) <= 2);
  }
  QCOMPARE(widget.findChildren<QWidget *>("buildModCard").size(), 4);
  QCOMPARE(widget.findChildren<QWidget *>("buildArcaneCard").size(), 2);
  QVERIFY(QFile::exists(":/resources/mod-slots/empty.png"));
  for (const QString &rarity :
       {"common", "uncommon", "rare", "legendary", "empty"}) {
    QVERIFY2(QFile::exists(":/resources/arcane-frames/" + rarity + ".png"),
             qPrintable(rarity));
  }
  QVERIFY(QFile::exists(":/resources/polarities/universal.png"));
  const QImage universal(":/resources/polarities/universal.png");
  QVERIFY(!universal.isNull());
  bool hasVisiblePixel = false;
  for (int y = 0; y < universal.height() && !hasVisiblePixel; ++y) {
    for (int x = 0; x < universal.width(); ++x) {
      if (universal.pixelColor(x, y).alpha() != 0) {
        hasVisiblePixel = true;
        break;
      }
    }
  }
  QVERIFY(hasVisiblePixel);

  const QString capture = qEnvironmentVariable("WFGUI_EMPTY_TOPOLOGY_CAPTURE");
  if (!capture.isEmpty()) {
    QVERIFY2(widget.grab().save(capture), qPrintable(capture));
  }
}

QTEST_MAIN(BuildTopologyWidgetTest)

#include "build_topology_widget_test.moc"
