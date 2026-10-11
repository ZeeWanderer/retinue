#include <QJsonArray>
#include <QLabel>
#include <QListView>
#include <QMenu>
#include <QLocalServer>
#include <QPushButton>
#include <QScopeGuard>
#include <QScrollArea>
#include <QTemporaryDir>
#include <QTimer>
#include <QtTest>

#include "app_controller.h"
#include "build_discover_widget.h"
#include "daemon_client.h"

class BuildDiscoverWidgetTest final : public QObject {
  Q_OBJECT

private slots:
  void rendersBuildCardsAndNotesInOneScrollArea();
};

void BuildDiscoverWidgetTest::rendersBuildCardsAndNotesInOneScrollArea() {
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
  BuildDiscoverWidget widget(&controller);
  widget.resize(1100, 700);
  widget.show();

  auto *client = controller.findChild<DaemonClient *>();
  QVERIFY(client);
  widget.ensureLoaded();
  client->buildSourceReady(
      QJsonObject{{"op", "build_search"},
                  {"query", ""},
                  {"class", "all"},
                  {"limit", 40}},
      QJsonObject{{"items", QJsonArray{QJsonObject{{"canonical_id", "/item"},
                                                   {"external_id", 100},
                                                   {"name", "Test Rifle"},
                                                   {"class", "primary"}}}}});
  QCoreApplication::processEvents();

  client->buildSourceReady(
      QJsonObject{{"op", "build_list"},
                  {"item", "/item"},
                  {"query", ""},
                  {"scope", "public"},
                  {"ordering", "score"},
                  {"limit", 50},
                  {"offset", 0},
                  {"refresh", false}},
      QJsonObject{
          {"builds", QJsonArray{QJsonObject{
                         {"identity", QJsonObject{{"external_id", 300}}},
                         {"title", "Test build"},
                         {"author", QJsonObject{{"username", "Tester"}}},
                         {"score", 10},
                         {"formas", 2}}}}});
  QCoreApplication::processEvents();
  QCOMPARE(controller.sourceBuildsError(), QString());
  QCOMPARE(controller.sourceBuilds()->rowCount(), 1);
  auto *builds = widget.findChild<QListView *>("buildSourceList");
  QVERIFY(builds);
  QCOMPARE(builds->currentIndex().data().toString(), QString("Test build"));

  const QJsonObject topology{
      {"regions", QJsonArray{QJsonObject{
                      {"id", "mods"},
                      {"label", "Mods"},
                      {"columns", 1},
                      {"slots", QJsonArray{QJsonObject{{"id", "mod-1"},
                                                       {"label", "Mod 1"},
                                                       {"role", "mod"},
                                                       {"build_slot", 1}}}}}}}};
  client->buildSourceReady(
      QJsonObject{
          {"op", "build_detail"}, {"build_id", 300}, {"refresh", false}},
      QJsonObject{{"identity", QJsonObject{{"external_id", 300}}},
                  {"fingerprint", "revision-fingerprint"},
                  {"content", QJsonObject{{"slots", QJsonArray{}}}},
                  {"metadata", QJsonObject{{"title", "Test build"},
                                           {"description", "Build **notes**"}}},
                  {"presentation",
                   QJsonObject{{"topology", topology},
                               {"upgrades", QJsonArray{QJsonObject{
                                                {"source_slot", 1},
                                                {"topology_slot", "mod-1"},
                                                {"name", "Serration"}}}}}}});
  QCoreApplication::processEvents();

  QCOMPARE(controller.buildRevision(300)
               .value("metadata")
               .toObject()
               .value("description")
               .toString(),
           QString("Build **notes**"));

  auto *scroll = widget.findChild<QScrollArea *>("buildRevisionScroll");
  auto *topologyWidget = widget.findChild<QWidget *>("buildTopology");
  auto *note = widget.findChild<QLabel *>("buildNote");
  auto *state = widget.findChild<QLabel *>("buildWorkspaceState");
  QVERIFY(scroll);
  QVERIFY(topologyWidget);
  QVERIFY(note);
  QVERIFY(state);
  QCOMPARE(state->text(), QString("Revision revision-fin"));
  QVERIFY(scroll->widget()->isAncestorOf(topologyWidget));
  QVERIFY(scroll->widget()->isAncestorOf(note));
  QTRY_COMPARE(note->text(), QString("Build **notes**"));
  QVERIFY(!note->isHidden());
  QTRY_VERIFY(widget.findChild<QWidget *>("buildModCard") != nullptr);
  const QWidget *mod = widget.findChild<QWidget *>("buildModCard");
  QCOMPARE(mod->property("upgradeName").toString(), QString("Serration"));

  QJsonObject group{{"id", "group-1"}, {"name", "Rifle builds"}, {"definition_id", "/item"},
                    {"revision", 1}, {"members", QJsonArray{}}};
  client->buildSourceReady({{"op", "build_group_list"}}, {{"groups", QJsonArray{group}}});
  QSignalSpy navigation(&widget, &BuildDiscoverWidget::groupRequested);
  QTimer::singleShot(0, [] {
    auto *menu = qobject_cast<QMenu *>(QApplication::activePopupWidget());
    QVERIFY(menu);
    menu->setActiveAction(menu->actions().first());
    QTest::keyClick(menu, Qt::Key_Return);
  });
  auto buttons = widget.findChildren<QPushButton *>();
  auto add = std::find_if(buttons.begin(), buttons.end(), [](auto *button) { return button->text() == "Add to group"; });
  QVERIFY(add != buttons.end());
  (*add)->click();
  group.insert("revision", 2);
  client->buildSourceReady({{"op", "build_group_add_source"}, {"group_id", "group-1"}}, group);
  QCOMPARE(navigation.count(), 0);
  QCOMPARE(state->text(), QString("Added to Rifle builds"));
  auto open = std::find_if(buttons.begin(), buttons.end(), [](auto *button) { return button->property("testId") == "buildAddedGroup"; });
  QVERIFY(open != buttons.end());
  QVERIFY((*open)->isVisible());
  (*open)->click();
  QCOMPARE(navigation.count(), 1);
  QCOMPARE(navigation.first().first().toString(), QString("group-1"));
}

QTEST_MAIN(BuildDiscoverWidgetTest)

#include "build_discover_widget_test.moc"
