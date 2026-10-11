#include <QApplication>
#include <QJsonDocument>
#include <QLabel>
#include <QLocalServer>
#include <QLocalSocket>
#include <QPointer>
#include <QScrollArea>
#include <QScrollBar>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>
#include <QToolButton>

#include "app_controller.h"
#include "market_order_card.h"
#include "market_widget.h"

class MarketWidgetTest final : public QObject {
  Q_OBJECT
private slots:
  void retainsCardsAndUpdatesActions();
  void variantQuotesRefreshWithoutDiscardingCachedData();
};

void MarketWidgetTest::retainsCardsAndUpdatesActions() {
  QLocalServer server;
  QVERIFY(server.listen(qEnvironmentVariable("RETINUE_DAEMON_SOCKET")));
  AppController controller;
  auto *client = controller.findChild<DaemonClient *>();
  QVERIFY(client);
  QTRY_VERIFY(server.hasPendingConnections());
  QScopedPointer<QLocalSocket> peer(server.nextPendingConnection());
  QTRY_VERIFY(peer->canReadLine());
  QJsonObject hello = QJsonDocument::fromJson(peer->readLine()).object();
  hello.insert("ok", true);
  hello.insert("compatible", true);
  peer->write(QJsonDocument(hello).toJson(QJsonDocument::Compact) + '\n');
  QTRY_VERIFY(client->connected());

  QJsonArray orders;
  QJsonArray items;
  for (int i = 0; i < 160; ++i) {
    const QString id = QString::number(i).rightJustified(3, '0');
    orders.append(QJsonObject{{"id", "order-" + id},
                              {"itemId", id},
                              {"visible", true},
                              {"type", "sell"},
                              {"quantity", 1},
                              {"platinum", 10}});
    items.append(QJsonObject{{"id", id}, {"name", "Item " + id}});
  }
  client->marketItemsDescribed(items, {});
  client->marketAccountReady("snapshot",
                             {{"authenticated", true}, {"orders", orders}});
  MarketWidget widget(&controller);
  widget.resize(1300, 800);
  widget.show();
  QTRY_COMPARE(widget.findChildren<QWidget *>("marketOrderCard").size(), 160);
  QList<QPointer<QWidget>> originalCards;
  for (QWidget *original : widget.findChildren<QWidget *>("marketOrderCard")) {
    originalCards.append(original);
  }
  auto *card = widget.findChild<QWidget *>("marketOrderCard");
  QSignalSpy destroyed(card, &QObject::destroyed);
  auto *focus = card->findChild<QToolButton *>("marketVisibility");
  focus->setFocus();
  QTRY_COMPARE(QApplication::focusWidget(), focus);
  auto *scroll = widget.findChild<QScrollArea *>("marketScroll");
  QTRY_VERIFY(scroll->verticalScrollBar()->maximum() > 500);
  scroll->verticalScrollBar()->setValue(500);
  controller.assetsChanged({"unrelated"});
  controller.marketQuotesChanged();
  QTest::qWait(80);
  QCOMPARE(destroyed.size(), 0);
  QCOMPARE(widget.findChild<QWidget *>("marketOrderCard"), card);
  QCOMPARE(widget.findChildren<QWidget *>("marketOrderCard").size(), originalCards.size());
  for (const auto &original : originalCards) {
    QVERIFY(!original.isNull());
  }
  QCOMPARE(QApplication::focusWidget(), focus);
  QCOMPARE(scroll->verticalScrollBar()->value(), 500);

  QJsonObject updated = orders.first().toObject();
  updated.insert("quantity", 8);
  updated.insert("visible", false);
  orders[0] = updated;
  client->marketAccountReady("snapshot",
                             {{"authenticated", true}, {"orders", orders}});
  QCOMPARE(destroyed.size(), 0);
  QVERIFY(!focus->isChecked());
  QTest::qWait(20);
  peer->readAll();
  card->findChild<QToolButton *>("marketAdd")->click();
  QTRY_VERIFY(peer->canReadLine());
  QJsonObject mutation;
  while (peer->canReadLine()) {
    const auto request = QJsonDocument::fromJson(peer->readLine()).object();
    if (request.value("op") == "market_order_update") {
      mutation = request;
    }
  }
  QCOMPARE(mutation.value("order_id").toString(), "order-000");
  QCOMPARE(mutation.value("patch").toObject().value("quantity").toInt(), 9);

  client->marketItemsDescribed({}, {});
  orders = QJsonArray{QJsonObject{{"id", "unknown"},
                                  {"itemId", "unknown"},
                                  {"type", "sell"},
                                  {"quantity", 1}}};
  client->marketAccountReady("snapshot",
                             {{"authenticated", true}, {"orders", orders}});
  QCOMPARE(widget.findChild<QLabel *>("marketOrderOwned")->text(), "-- owned");
  QCOMPARE(widget.findChild<QLabel *>("marketOrderName")->text(),
           "Loading item...");
  QVERIFY(widget.findChild<QLabel *>("marketWarning")->isHidden());
}

void MarketWidgetTest::variantQuotesRefreshWithoutDiscardingCachedData() {
  QLocalServer server;
  QVERIFY(server.listen(qEnvironmentVariable("RETINUE_DAEMON_SOCKET")));
  AppController controller;
  auto *client = controller.findChild<DaemonClient *>();
  QTRY_VERIFY(server.hasPendingConnections());
  QScopedPointer<QLocalSocket> peer(server.nextPendingConnection());
  QTRY_VERIFY(peer->canReadLine());
  QJsonObject hello = QJsonDocument::fromJson(peer->readLine()).object();
  hello.insert("ok", true);
  hello.insert("compatible", true);
  peer->write(QJsonDocument(hello).toJson(QJsonDocument::Compact) + '\n');
  QTRY_VERIFY(client->connected());
  const QJsonObject filters{{"rank", 0}};
  const QJsonObject cached{{"quote", QJsonObject{{"lowest_sell", 5}}}};
  client->marketVariantQuoteReady("hush", filters, cached);
  controller.requestMarketVariantQuote("hush", filters);
  QTRY_VERIFY(peer->canReadLine());
  QJsonObject request;
  QTRY_VERIFY(([&] {
    while (peer->canReadLine()) {
      const auto row = QJsonDocument::fromJson(peer->readLine()).object();
      if (row.value("op") == "market_quote_variant") {
        request = row;
      }
    }
    return !request.isEmpty();
  })());
  QVERIFY(request.value("refresh").toBool());
  QCOMPARE(controller.marketVariantQuote("hush", filters), cached);
  const QJsonObject fresh{{"quote", QJsonObject{{"lowest_sell", 7}}}};
  peer->write(QJsonDocument(QJsonObject{{"id", request.value("id")},
                                        {"ok", true},
                                        {"data", fresh}})
                  .toJson(QJsonDocument::Compact) +
              '\n');
  QTRY_COMPARE(controller.marketVariantQuote("hush", filters), fresh);
  QSignalSpy ready(&controller, &AppController::marketVariantQuoteReady);
  controller.requestMarketVariantQuote("hush", filters);
  const QJsonObject newer{{"quote", QJsonObject{{"lowest_sell", 8}}}};
  client->marketVariantQuoteReady("hush", filters, newer);
  QTRY_COMPARE(ready.size(), 2);
  QCOMPARE(ready.last().last().toJsonObject(), newer);
  QTest::qWait(20);
  QVERIFY(!peer->canReadLine());
  controller.requestMarketVariantQuote("hush", filters, true);
  QTRY_VERIFY(peer->canReadLine());
  request = QJsonDocument::fromJson(peer->readLine()).object();
  QCOMPARE(request.value("op").toString(), "market_quote_variant");
  QVERIFY(request.value("refresh").toBool());
}

int main(int argc, char **argv) {
  QTemporaryDir directory;
  qputenv("XDG_CACHE_HOME", directory.filePath("cache").toUtf8());
  qputenv("XDG_CONFIG_HOME", directory.filePath("config").toUtf8());
  qputenv("RETINUE_DAEMON_SOCKET", directory.filePath("daemon.sock").toUtf8());
  QApplication app(argc, argv);
  MarketWidgetTest test;
  return QTest::qExec(&test, argc, argv);
}

#include "market_widget_test.moc"
