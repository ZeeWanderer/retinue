#pragma once

#include <QJsonArray>
#include <QObject>
#include <QString>

class QNetworkAccessManager;
class QProcess;
class QTimer;
class QWebSocket;

class OverframeLogin final : public QObject {
  Q_OBJECT

public:
  explicit OverframeLogin(QObject *parent = nullptr);
  ~OverframeLogin() override;

  bool active() const;
  void start();
  void cancel();

signals:
  void cookiesReady(const QJsonArray &cookies);
  void failed(const QString &error);
  void activeChanged(bool active);

private:
  void pollDebugger();
  void connectDebugger(const QString &url);
  void requestCookies();
  void handleDebuggerMessage(const QString &message);
  void closeBrowser();
  void finish();
  void fail(const QString &error);
  void removeProfile();

  QNetworkAccessManager *network_;
  QProcess *process_;
  QTimer *discoveryTimer_;
  QTimer *startupTimer_;
  QTimer *cookieTimer_;
  QWebSocket *webSocket_;
  QString profilePath_;
  quint16 port_ = 0;
  int nextCommandId_ = 1;
  int cookieCommandId_ = 0;
  bool active_ = false;
  bool closing_ = false;
  bool discoveryPending_ = false;
};
