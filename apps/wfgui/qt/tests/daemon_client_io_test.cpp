#include <QApplication>
#include <QJsonDocument>
#include <QLocalServer>
#include <QLocalSocket>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>
#include <QTimer>

#include "daemon_client.h"

namespace {
struct Peer {
  QLocalServer server;
  DaemonClient client;
  QScopedPointer<QLocalSocket> socket;

  void send(QJsonObject message) {
    socket->write(QJsonDocument(message).toJson(QJsonDocument::Compact) + '\n');
  }

  bool accept() {
    if (!QTest::qWaitFor([this] { return server.hasPendingConnections(); })) {
      return false;
    }
    socket.reset(server.nextPendingConnection());
    if (!QTest::qWaitFor([this] { return socket->canReadLine(); })) {
      return false;
    }
    QJsonObject hello = QJsonDocument::fromJson(socket->readLine()).object();
    hello.insert("ok", true);
    hello.insert("compatible", true);
    send(hello);
    return QTest::qWaitFor([this] { return client.connected(); });
  }

  bool start() {
    if (!server.listen(qEnvironmentVariable("RETINUE_DAEMON_SOCKET"))) {
      return false;
    }
    client.start();
    if (!accept()) {
      return false;
    }
    for (int i = 0; i < 3; ++i) {
      QTest::qWait(10);
      while (socket->canReadLine()) {
        const auto request =
            QJsonDocument::fromJson(socket->readLine()).object();
        send({{"id", request.value("id")},
              {"ok", true},
              {"data", QJsonObject{}}});
      }
    }
    return true;
  }
};
} // namespace

class DaemonClientIoTest final : public QObject {
  Q_OBJECT
private slots:
  void fragmentedFramesAndBurstsYieldToEvents();
  void rejectsOversizedFrames();
  void stalledRequestsRecoverWithoutRepeatingMutations();
};

void DaemonClientIoTest::fragmentedFramesAndBurstsYieldToEvents() {
  Peer peer;
  QVERIFY(peer.start());
  QSignalSpy assets(&peer.client, &DaemonClient::assetRefreshed);
  const QJsonObject data{{"id", "large"},
                         {"padding", QString(4 * 1024 * 1024, 'x')}};
  const QByteArray frame =
      QJsonDocument(QJsonObject{{"event", "asset"}, {"data", data}})
          .toJson(QJsonDocument::Compact);
  peer.socket->write(frame.left(35));
  QTest::qWait(10);
  QCOMPARE(assets.size(), 0);
  peer.socket->write(frame.mid(35));
  QTest::qWait(20);
  QCOMPARE(assets.size(), 0);
  peer.socket->write("\n");
  QTRY_COMPARE(assets.size(), 1);
  QCOMPARE(assets.first().first().toJsonObject(), data);
  assets.clear();

  int handledWhenTimerRan = -1;
  bool scheduled = false;
  connect(&peer.client, &DaemonClient::assetRefreshed, this, [&] {
    if (!scheduled) {
      scheduled = true;
      QTimer::singleShot(0, this, [&] { handledWhenTimerRan = assets.size(); });
    }
  });
  QByteArray burst;
  for (int i = 0; i < 100; ++i) {
    burst += QJsonDocument(QJsonObject{{"event", "asset"},
                                       {"data", QJsonObject{{"id", i}}}})
                 .toJson(QJsonDocument::Compact) +
             '\n';
  }
  peer.socket->write(burst);
  QTRY_COMPARE(assets.size(), 100);
  QVERIFY(handledWhenTimerRan > 0);
  QVERIFY(handledWhenTimerRan < 100);
  QVERIFY(peer.client.connected());
  QVERIFY(peer.client.findChild<QLocalSocket *>()->readBufferSize() <=
          256 * 1024);
}

void DaemonClientIoTest::rejectsOversizedFrames() {
  Peer peer;
  QVERIFY(peer.start());
  peer.socket->write(QByteArray(64 * 1024 * 1024 + 1, 'x'));
  QTRY_VERIFY_WITH_TIMEOUT(!peer.client.connected(), 10'000);
  QVERIFY(peer.client.status().contains("frame limit"));
}

void DaemonClientIoTest::stalledRequestsRecoverWithoutRepeatingMutations() {
  Peer peer;
  QVERIFY(peer.start());
  auto *deadline = peer.client.findChild<QTimer *>("replyDeadline");
  QVERIFY(deadline);
  QTRY_VERIFY(!deadline->isActive());
  peer.client.planBuildGroup("group", 1);
  QVERIFY(deadline->interval() > 590'000);
  peer.client.requestActivity();
  QVERIFY(deadline->interval() <= 120'000);
  QVERIFY(deadline->interval() > 119'000);
  QSignalSpy failed(&peer.client, &DaemonClient::buildSourceFailed);
  deadline->start(1);
  QTRY_VERIFY(!peer.client.connected());
  QCOMPARE(failed.size(), 1);
  QVERIFY(failed.first().last().toString().contains("result is unknown"));
  QVERIFY(peer.client.status().contains("Timed out"));
  QVERIFY(peer.accept());
  QTRY_VERIFY(peer.socket->canReadLine());
  bool activity = false;
  bool groups = false;
  QTest::qWait(20);
  while (peer.socket->canReadLine()) {
    const QString op = QJsonDocument::fromJson(peer.socket->readLine())
                           .object()
                           .value("op")
                           .toString();
    QVERIFY(op != "build_group_plan");
    activity |= op == "activity_view";
    groups |= op == "build_group_list";
  }
  QVERIFY(activity);
  QVERIFY(groups);
}

int main(int argc, char **argv) {
  QTemporaryDir directory;
  qputenv("RETINUE_DAEMON_SOCKET", directory.filePath("daemon.sock").toUtf8());
  QApplication app(argc, argv);
  DaemonClientIoTest test;
  return QTest::qExec(&test, argc, argv);
}

#include "daemon_client_io_test.moc"
