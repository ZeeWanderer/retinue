#include <QJsonArray>
#include <QComboBox>
#include <QAbstractItemView>
#include <QCheckBox>
#include <QFrame>
#include <QFile>
#include <QFontDatabase>
#include <QJsonDocument>
#include <QLabel>
#include <QLocalServer>
#include <QScopeGuard>
#include <QScrollArea>
#include <QPushButton>
#include <QPainter>
#include <QScrollBar>
#include <QStyleOptionViewItem>
#include <QStyledItemDelegate>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QVBoxLayout>
#include <QtTest>

#include "app_controller.h"
#include "build_groups_widget.h"
#include "daemon_client.h"
#include "style_loader.h"

namespace {
template <typename T> T *findByTestId(QWidget &widget, const char *id) {
  for (T *child : widget.findChildren<T *>()) {
    if (child->property("testId").toString() == id) {
      return child;
    }
  }
  return nullptr;
}

int checkPixels(QComboBox *combo, int row, bool hovered) {
  QImage image(500, 32, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  QPainter painter(&image);
  QStyleOptionViewItem option;
  option.initFrom(combo->view());
  option.widget = combo->view();
  option.rect = image.rect();
  option.state.setFlag(QStyle::State_Selected, hovered);
  combo->itemDelegate()->paint(&painter, option, combo->model()->index(row, 0));
  painter.end();
  int pixels = 0;
  for (int y = 0; y < image.height(); ++y) {
    for (int x = 3; x < 19; ++x) {
      const QColor color = image.pixelColor(x, y);
      if (color.alpha() > 128 && color.red() > 180 && color.green() > 180 && color.blue() > 180) ++pixels;
    }
  }
  return pixels;
}
} // namespace

class BuildGroupsWidgetTest final : public QObject {
  Q_OBJECT

private slots:
  void initTestCase();
  void showsFocusedEmptyState();
  void rendersPlanAndEveryMember();
  void capturesPlanFixture();
};

void BuildGroupsWidgetTest::initTestCase() {
  QVERIFY(!QPixmap(":/resources/ui/chevron-down.png").isNull());
  for (const char *weight : {"Regular", "Medium", "Bold", "Light"}) {
    const QString path = QString(":/assets/Roboto-%1.ttf").arg(weight);
    const int id = QFontDatabase::addApplicationFont(path);
    QVERIFY2(id >= 0, qPrintable(path));
    QVERIFY(QFontDatabase::applicationFontFamilies(id).contains("Roboto"));
  }
  qApp->setStyleSheet(wfgui::applicationStyleSheet());
}

void BuildGroupsWidgetTest::showsFocusedEmptyState() {
  AppController controller;
  BuildGroupsWidget widget(&controller);
  widget.resize(1000, 700);
  widget.show();

  auto *client = controller.findChild<DaemonClient *>();
  QVERIFY(client);
  client->buildSourceReady(QJsonObject{{"op", "build_group_list"}},
                           QJsonObject{{"groups", QJsonArray{}}});
  QCoreApplication::processEvents();

  auto *empty = widget.findChild<QWidget *>("buildGroupEmpty");
  auto *editor = widget.findChild<QWidget *>("buildGroupEditor");
  auto *title = findByTestId<QLabel>(widget, "buildGroupEmptyTitle");
  auto *equipment =
      findByTestId<QPushButton>(widget, "buildGroupEmptyEquipment");
  auto *discover =
      findByTestId<QPushButton>(widget, "buildGroupEmptyDiscover");
  QVERIFY(empty);
  QVERIFY(editor);
  QVERIFY(title);
  QVERIFY(equipment);
  QVERIFY(discover);
  QVERIFY(empty->isVisible());
  QVERIFY(!editor->isVisible());
  QCOMPARE(title->text(), QString("No build groups"));

  QSignalSpy equipmentSpy(&widget, &BuildGroupsWidget::equipmentRequested);
  QSignalSpy discoverSpy(&widget, &BuildGroupsWidget::discoverRequested);
  QTest::mouseClick(equipment, Qt::LeftButton);
  QTest::mouseClick(discover, Qt::LeftButton);
  QCOMPARE(equipmentSpy.count(), 1);
  QCOMPARE(discoverSpy.count(), 1);
  QCOMPARE(equipmentSpy.takeFirst().at(0).toString(), QString());
  QCOMPARE(discoverSpy.takeFirst().at(0).toString(), QString());
}

void BuildGroupsWidgetTest::rendersPlanAndEveryMember() {
  QTemporaryDir directory;
  QLocalServer server;
  QVERIFY(server.listen(directory.filePath("daemon.sock")));
  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", server.fullServerName().toUtf8());
  const auto restore = qScopeGuard([oldSocket] { qputenv("RETINUE_DAEMON_SOCKET", oldSocket); });
  AppController controller;
  QTRY_VERIFY(server.hasPendingConnections());
  QVERIFY(server.nextPendingConnection());
  BuildGroupsWidget widget(&controller);
  widget.resize(1100, 850);
  widget.show();
  auto *client = controller.findChild<DaemonClient *>();
  const QJsonObject slot{{"id", "mod-1"}, {"label", "Mod 1"}, {"role", "mod"}, {"planner", true}};
  const QJsonObject topology{{"regions", QJsonArray{QJsonObject{
      {"id", "mods"}, {"label", "Mods"}, {"columns", 4}, {"slots", QJsonArray{slot}}}}}};
  const QJsonObject baseline{{"instance_id", "copy-1"}, {"fingerprint", "physical"},
      {"topology", topology}, {"forma_count", 4},
      {"effective_polarities", QJsonArray{QJsonObject{{"slot_id", "mod-1"}, {"polarity", "none"}}}}};
  QJsonArray members;
  QJsonArray loadouts;
  for (const QString &name : {QString("Galvanized Hell"), QString("Galvanized Savvy")}) {
    const QJsonObject upgrade{{"topology_slot", "mod-1"}, {"name", name}, {"rank", 10},
        {"mod_variant", "galvanized"}, {"cost", 12}, {"mod_polarity", "madurai"}};
    members.append(QJsonObject{{"id", name}, {"name", name}, {"kind", "source_revision"},
        {"snapshot", QJsonObject{{"metadata", QJsonObject{{"description", "Build notes"}}},
          {"presentation", QJsonObject{{"topology", topology}, {"upgrades", QJsonArray{upgrade}}}}}}});
    auto finalMod = upgrade;
    finalMod.insert("effective_drain", 6);
    finalMod.insert("slot_polarity", "madurai");
    finalMod.insert("polarity_state", "matched");
    loadouts.append(QJsonObject{{"member_id", name}, {"capacity", 60}, {"drain", 6},
        {"remaining_capacity", 54}, {"upgrade_slots", QJsonArray{finalMod}}});
  }
  QJsonObject group{{"id", "group-1"}, {"name", "Boar"}, {"definition_id", "/boar"},
      {"instance_id", "copy-1"}, {"revision", 1}, {"baseline", baseline}, {"members", members}};
  client->buildSourceReady({{"op", "build_group_list"}}, {{"groups", QJsonArray{group}}});
  widget.selectGroup("group-1");
  client->buildSourceReady({{"op", "build_group_get"}, {"group_id", "group-1"}}, group);
  QJsonObject result{{"status", "ready"}, {"group_revision", 1}, {"target_fingerprint", "physical"},
      {"forma_requirements", QJsonObject{{"standard", 1}}}, {"builds", loadouts},
      {"final_polarities", QJsonArray{QJsonObject{{"slot_id", "mod-1"}, {"polarity", "madurai"}, {"changed", true}}}},
      {"operations", QJsonArray{QJsonObject{{"action", "polarize"}, {"label", "Mod 1"},
          {"before", "none"}, {"polarity", "madurai"}, {"forma", "standard"}}}}};
  client->buildSourceReady({{"op", "build_group_plan"}, {"group_id", "group-1"}}, result);
  auto *planned = findByTestId<QPushButton>(widget, "buildPlanned");
  QVERIFY(planned->isChecked());
  auto *selector = widget.findChild<QComboBox *>("buildGroupMembers");
  QCOMPARE(selector->count(), 2);
  QVERIFY(!widget.findChild<QCheckBox *>("buildKeepExactSlots")->isChecked());
  for (int index = 0; index < 2; ++index) {
    selector->setCurrentIndex(index);
    QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
    auto *card = widget.findChild<QWidget *>("buildModCard");
    QVERIFY(card);
    QCOMPARE(card->property("upgradeName").toString(), selector->currentText());
    QCOMPARE(card->property("modVariant").toString(), QString("galvanized"));
    QCOMPARE(card->property("effectiveDrain").toInt(), 6);
    QCOMPARE(card->property("polarityState").toString(), QString("matched"));
    QCOMPARE(selector->itemData(index, Qt::ToolTipRole).toString(), selector->currentText());
    QVERIFY(checkPixels(selector, index, false) > 8);
    QCOMPARE(checkPixels(selector, 1 - index, true), 0);
  }
  selector->setCurrentIndex(0);
  selector->showPopup();
  QCoreApplication::processEvents();
  QTest::keyClick(selector->view(), Qt::Key_Down);
  QCOMPARE(selector->currentIndex(), 0);
  QVERIFY(checkPixels(selector, 0, false) > 8);
  QCOMPARE(checkPixels(selector, 1, true), 0);
  QTest::keyClick(selector->view(), Qt::Key_Return);
  QCOMPARE(selector->currentIndex(), 1);
  QCOMPARE(widget.findChild<QLabel *>("buildCapacity")->text(), QString("6 / 60 capacity used · 54 free"));
  const auto cells = widget.findChildren<QFrame *>("buildPolarityCell");
  QCOMPARE(cells.size(), 2);
  QCOMPARE(cells.first()->property("polarity").toString(), QString("none"));
  QCOMPARE(cells.last()->property("polarity").toString(), QString("madurai"));
  auto *scroll = widget.findChild<QScrollArea *>("buildRevisionScroll");
  QVERIFY(scroll->widget()->isAncestorOf(widget.findChild<QWidget *>("buildTopology")));
  QVERIFY(scroll->widget()->isAncestorOf(widget.findChild<QLabel *>("buildNote")));
  findByTestId<QPushButton>(widget, "buildOriginal")->click();
  QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
  QCOMPARE(widget.findChild<QWidget *>("buildModCard")->property("effectiveDrain").toInt(), 12);

  result.insert("final_polarities", baseline.value("effective_polarities"));
  result.insert("operations", QJsonArray{});
  result.insert("forma_requirements", QJsonObject{});
  client->buildSourceReady({{"op", "build_group_plan"}, {"group_id", "group-1"}}, result);
  QCOMPARE(widget.findChildren<QFrame *>("buildPolarityCell").size(), 0);
  QCOMPARE(widget.findChild<QLabel *>("buildPlanSummary")->text(), QString("No polarity changes needed"));

  auto *actions = widget.findChild<QWidget *>("buildGroupActions");
  auto *edge = widget.findChild<QFrame *>("buildScrollEdge");
  auto *notes = widget.findChild<QLabel *>("buildNote");
  QVERIFY(actions);
  QVERIFY(edge);
  QVERIFY(!edge->isVisible());
  QVERIFY(edge->testAttribute(Qt::WA_TransparentForMouseEvents));
  notes->setMinimumHeight(1200);
  QTRY_VERIFY(scroll->verticalScrollBar()->maximum() > 0);
  scroll->verticalScrollBar()->setValue(scroll->verticalScrollBar()->maximum());
  QTRY_VERIFY(edge->isVisible());
  QCOMPARE(scroll->mapTo(&widget, QPoint(0, scroll->height())).y(), widget.height());
  QVERIFY(notes->mapTo(&widget, QPoint(0, notes->height())).y() < actions->mapTo(&widget, QPoint()).y());
  QVERIFY(actions->width() < scroll->viewport()->width());
  group.insert("members", QJsonArray{});
  group.insert("revision", 2);
  client->buildSourceReady({{"op", "build_group_get"}, {"group_id", "group-1"}}, group);
  QVERIFY(!scroll->isVisible());
  QVERIFY(!edge->isVisible());
  QVERIFY(!findByTestId<QPushButton>(widget, "buildGroupCalculate")->isEnabled());
}

void BuildGroupsWidgetTest::capturesPlanFixture() {
  const QString fixture = qEnvironmentVariable("WFGUI_BUILD_PLAN_FIXTURE");
  if (fixture.isEmpty()) QSKIP("No live plan capture requested");
  QFile file(fixture);
  QVERIFY(file.open(QIODevice::ReadOnly));
  const QJsonObject data = QJsonDocument::fromJson(file.readAll()).object();
  const QJsonObject group = data.value("group").toObject();
  QVERIFY(!group.isEmpty());
  QTemporaryDir directory;
  QLocalServer server;
  QVERIFY(server.listen(directory.filePath("daemon.sock")));
  const QByteArray oldSocket = qgetenv("RETINUE_DAEMON_SOCKET");
  qputenv("RETINUE_DAEMON_SOCKET", server.fullServerName().toUtf8());
  const auto restore = qScopeGuard([oldSocket] { qputenv("RETINUE_DAEMON_SOCKET", oldSocket); });
  AppController controller;
  QTRY_VERIFY(server.hasPendingConnections());
  QVERIFY(server.nextPendingConnection());
  QWidget host;
  host.setObjectName("appRoot");
  auto *layout = new QVBoxLayout(&host);
  auto *widget = new BuildGroupsWidget(&controller);
  layout->addWidget(widget);
  host.resize(1300, 950);
  host.show();
  auto *client = controller.findChild<DaemonClient *>();
  client->buildSourceReady({{"op", "build_group_list"}}, {{"groups", QJsonArray{group}}});
  const QString id = group.value("id").toString();
  widget->selectGroup(id);
  client->buildSourceReady({{"op", "build_group_get"}, {"group_id", id}}, group);
  client->buildSourceReady({{"op", "build_group_plan"}, {"group_id", id}},
                           group.value("plan_result").toObject());
  client->assetsResolved(data.value("assets").toArray());
  QTest::qWait(1500);
  auto *planned = findByTestId<QPushButton>(*widget, "buildPlanned");
  if (group.value("plan_result").toObject().value("status").toString() == "ready") {
    QVERIFY(planned->isChecked());
  }
  QVERIFY(host.grab().save(qEnvironmentVariable("WFGUI_BUILD_PLAN_CAPTURE")));
  if (const QString path = qEnvironmentVariable("WFGUI_BUILD_SCROLL_CAPTURE"); !path.isEmpty()) {
    auto *bar = widget->findChild<QScrollArea *>("buildRevisionScroll")->verticalScrollBar();
    bar->setValue(bar->maximum());
    QTest::qWait(50);
    QVERIFY(host.grab().save(path));
  }
  if (const QString path = qEnvironmentVariable("WFGUI_BUILD_POPUP_CAPTURE"); !path.isEmpty()) {
    auto *selector = widget->findChild<QComboBox *>("buildGroupMembers");
    selector->setCurrentIndex(qMin(1, selector->count() - 1));
    selector->showPopup();
    selector->view()->setCurrentIndex(selector->model()->index(0, 0));
    QTest::qWait(50);
    QVERIFY(selector->view()->window()->grab().save(path));
    selector->hidePopup();
  }
}

QTEST_MAIN(BuildGroupsWidgetTest)

#include "build_groups_widget_test.moc"
