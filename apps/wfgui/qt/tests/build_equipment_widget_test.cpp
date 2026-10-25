#include <QImage>
#include <QJsonArray>
#include <QLocalServer>
#include <QMenu>
#include <QPushButton>
#include <QScopeGuard>
#include <QSignalSpy>
#include <QSplitter>
#include <QTemporaryDir>
#include <QTimer>
#include <QToolButton>
#include <QVariantAnimation>
#include <QtTest>

#include "app_controller.h"
#include "build_equipment_widget.h"
#include "daemon_client.h"

class BuildEquipmentWidgetTest final : public QObject {
  Q_OBJECT

private slots:
  void animatesEquipmentRailWithVisibleGlyph();
  void addingConfigurationKeepsEquipmentOpen();
};

void BuildEquipmentWidgetTest::animatesEquipmentRailWithVisibleGlyph() {
  AppController controller;
  BuildEquipmentWidget widget(&controller);
  widget.resize(1000, 700);
  widget.show();
  QTest::qWait(10);

  auto *splitter = widget.findChild<QSplitter *>("buildEquipmentSplit");
  auto *toggle = widget.findChild<QToolButton *>("compactTool");
  auto *animation = widget.findChild<QVariantAnimation *>("buildRailAnimation");
  QVERIFY(splitter);
  QVERIFY(toggle);
  QVERIFY(animation);
  QCOMPARE(animation->duration(), 160);
  QVERIFY(splitter->widget(0)->isVisible());
  QVERIFY(splitter->sizes().at(0) >= 220);

  const QImage icon = toggle->icon().pixmap(16, 16).toImage();
  bool hasLightPixel = false;
  for (int y = 0; y < icon.height() && !hasLightPixel; ++y) {
    for (int x = 0; x < icon.width(); ++x) {
      const QColor pixel = icon.pixelColor(x, y);
      if (pixel.alpha() > 0 && pixel.lightness() > 150) {
        hasLightPixel = true;
        break;
      }
    }
  }
  QVERIFY(hasLightPixel);

  QTest::mouseClick(toggle, Qt::LeftButton);
  QTRY_VERIFY_WITH_TIMEOUT(!splitter->widget(0)->isVisible(), 400);
  QCOMPARE(splitter->sizes().at(0), 0);

  QTest::mouseClick(toggle, Qt::LeftButton);
  QTRY_VERIFY_WITH_TIMEOUT(splitter->widget(0)->isVisible(), 400);
  QTRY_VERIFY_WITH_TIMEOUT(splitter->sizes().at(0) >= 220, 400);
}

void BuildEquipmentWidgetTest::addingConfigurationKeepsEquipmentOpen() {
  QTemporaryDir directory;
  QLocalServer server;
  QVERIFY(server.listen(directory.filePath("daemon.sock")));
  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", server.fullServerName().toUtf8());
  const auto restore = qScopeGuard([oldSocket] { qputenv("RETINUE_DAEMON_SOCKET", oldSocket); });
  AppController controller;
  QTRY_VERIFY(server.hasPendingConnections());
  QVERIFY(server.nextPendingConnection());
  BuildEquipmentWidget widget(&controller);
  widget.resize(1100, 800);
  widget.show();
  auto *client = controller.findChild<DaemonClient *>();
  client->playerViewReady("build_equipment", {
      {"definitions", QJsonArray{QJsonObject{{"id", "/item"}, {"name", "Rifle"}, {"class", "primary"}}}},
      {"instances", QJsonArray{QJsonObject{{"definition_id", "/item"}, {"instance_id", "copy-1"},
          {"configs", QJsonArray{QJsonObject{{"config_index", 0}, {"name", "Config A"}}}}}}}});
  QJsonObject group{{"id", "group-1"}, {"name", "Rifle builds"}, {"definition_id", "/item"},
                    {"instance_id", "copy-1"}, {"revision", 1}, {"members", QJsonArray{}}};
  client->buildSourceReady({{"op", "build_group_list"}}, {{"groups", QJsonArray{group}}});
  widget.selectDefinition("/item", "copy-1");
  QSignalSpy navigation(&widget, &BuildEquipmentWidget::groupRequested);
  auto buttons = widget.findChildren<QPushButton *>();
  auto add = std::find_if(buttons.begin(), buttons.end(), [](auto *button) {
    return button->text() == "Add current configuration";
  });
  QVERIFY(add != buttons.end());
  QVERIFY((*add)->isEnabled());
  QTimer::singleShot(0, [] {
    auto *menu = qobject_cast<QMenu *>(QApplication::activePopupWidget());
    QVERIFY(menu);
    menu->setActiveAction(menu->actions().first());
    QTest::keyClick(menu, Qt::Key_Return);
  });
  (*add)->click();
  group.insert("revision", 2);
  client->buildSourceReady({{"op", "build_group_add_config"}, {"group_id", "group-1"}}, group);
  QCOMPARE(navigation.count(), 0);
  auto open = std::find_if(buttons.begin(), buttons.end(), [](auto *button) {
    return button->text() == "Open Rifle builds";
  });
  QVERIFY(open != buttons.end());
  QVERIFY((*open)->isVisible());
  (*open)->click();
  QCOMPARE(navigation.count(), 1);
  QCOMPARE(navigation.first().first().toString(), QString("group-1"));
}

QTEST_MAIN(BuildEquipmentWidgetTest)

#include "build_equipment_widget_test.moc"
