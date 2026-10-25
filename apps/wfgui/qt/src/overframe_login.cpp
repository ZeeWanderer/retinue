#include "overframe_login.h"

#include <QAbstractSocket>
#include <QDir>
#include <QFileInfo>
#include <QHostAddress>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QProcess>
#include <QStandardPaths>
#include <QTcpServer>
#include <QTemporaryDir>
#include <QTimer>
#include <QUrl>
#include <QWebSocket>

#include <optional>

namespace {
constexpr int DebuggerPollMs = 250;
constexpr int CookiePollMs = 500;
constexpr int BrowserStartupTimeoutMs = 15'000;

struct Browser {
  QString program;
  QStringList prefix;
};

std::optional<Browser> nativeBrowser() {
  static const QStringList Candidates = {
      "brave-browser",         "brave",         "google-chrome-stable",
      "google-chrome",         "chromium",      "chromium-browser",
      "microsoft-edge-stable", "microsoft-edge"};
  for (const QString &candidate : Candidates) {
    const QString path = QStandardPaths::findExecutable(candidate);
    if (!path.isEmpty()) {
      return Browser{.program = path, .prefix = {}};
    }
  }
  return std::nullopt;
}

bool flatpakInstalled(const QString &id) {
  const QString suffix = "/app/" + id + "/current/active";
  return QFileInfo::exists(QDir::homePath() + "/.local/share/flatpak" +
                           suffix) ||
         QFileInfo::exists("/var/lib/flatpak" + suffix);
}

std::optional<Browser> flatpakBrowser(const QString &profilePath) {
  const QString flatpak = QStandardPaths::findExecutable("flatpak");
  if (flatpak.isEmpty()) {
    return std::nullopt;
  }
  static const QStringList Candidates = {
      "com.brave.Browser", "com.google.Chrome", "org.chromium.Chromium",
      "com.microsoft.Edge"};
  for (const QString &id : Candidates) {
    if (flatpakInstalled(id)) {
      return Browser{.program = flatpak,
                     .prefix = {"run", "--filesystem=" + profilePath, id}};
    }
  }
  return std::nullopt;
}

bool overframeDomain(QString domain) {
  domain = domain.toLower();
  return domain == "overframe.gg" || domain == ".overframe.gg";
}
} // namespace

OverframeLogin::OverframeLogin(QObject *parent)
    : QObject(parent), network_(new QNetworkAccessManager(this)),
      process_(new QProcess(this)), discoveryTimer_(new QTimer(this)),
      startupTimer_(new QTimer(this)), cookieTimer_(new QTimer(this)),
      webSocket_(new QWebSocket("http://localhost",
                                QWebSocketProtocol::VersionLatest, this)) {
  discoveryTimer_->setInterval(DebuggerPollMs);
  startupTimer_->setSingleShot(true);
  cookieTimer_->setInterval(CookiePollMs);

  connect(discoveryTimer_, &QTimer::timeout, this,
          &OverframeLogin::pollDebugger);
  connect(startupTimer_, &QTimer::timeout, this,
          [this] { fail("Could not connect to the sign-in browser"); });
  connect(cookieTimer_, &QTimer::timeout, this,
          &OverframeLogin::requestCookies);
  connect(webSocket_, &QWebSocket::connected, this, [this] {
    discoveryTimer_->stop();
    startupTimer_->stop();
    cookieTimer_->start();
    requestCookies();
  });
  connect(webSocket_, &QWebSocket::textMessageReceived, this,
          &OverframeLogin::handleDebuggerMessage);
  connect(webSocket_, &QWebSocket::disconnected, this, [this] {
    if (active_ && !closing_) {
      fail("Overframe sign-in window closed");
    }
  });
  connect(process_, &QProcess::errorOccurred, this,
          [this](QProcess::ProcessError error) {
            if (active_ && error == QProcess::FailedToStart) {
              fail("Could not start a compatible browser");
            }
          });
  connect(process_, qOverload<int, QProcess::ExitStatus>(&QProcess::finished),
          this, [this] {
            if (!active_) {
              return;
            }
            if (closing_) {
              finish();
            } else {
              fail("Overframe sign-in window closed");
            }
          });
}

OverframeLogin::~OverframeLogin() {
  closing_ = true;
  webSocket_->abort();
  if (process_->state() != QProcess::NotRunning) {
    process_->terminate();
    if (!process_->waitForFinished(1000)) {
      process_->kill();
      process_->waitForFinished(1000);
    }
  }
  removeProfile();
}

bool OverframeLogin::active() const { return active_; }

void OverframeLogin::start() {
  if (active_) {
    return;
  }

  QTemporaryDir profile(QDir::tempPath() + "/wfgui-overframe-XXXXXX");
  if (!profile.isValid()) {
    emit failed("Could not create a temporary browser profile");
    return;
  }
  profile.setAutoRemove(false);
  profilePath_ = profile.path();

  std::optional<Browser> browser = nativeBrowser();
  if (!browser.has_value()) {
    browser = flatpakBrowser(profilePath_);
  }
  if (!browser.has_value()) {
    removeProfile();
    emit failed("No supported Chromium browser found");
    return;
  }

  QTcpServer portReservation;
  if (!portReservation.listen(QHostAddress::LocalHost, 0)) {
    removeProfile();
    emit failed("Could not reserve a local browser control port");
    return;
  }
  port_ = portReservation.serverPort();
  portReservation.close();

  QStringList arguments = browser->prefix;
  arguments.append({"--user-data-dir=" + profilePath_,
                    "--remote-debugging-address=127.0.0.1",
                    QString("--remote-debugging-port=%1").arg(port_),
                    "--remote-allow-origins=http://localhost", "--no-first-run",
                    "--no-default-browser-check", "--disable-sync",
                    "--new-window", "https://overframe.gg/account/"});

  active_ = true;
  closing_ = false;
  discoveryPending_ = false;
  nextCommandId_ = 1;
  cookieCommandId_ = 0;
  emit activeChanged(true);
  process_->start(browser->program, arguments);
  discoveryTimer_->start();
  startupTimer_->start(BrowserStartupTimeoutMs);
  pollDebugger();
}

void OverframeLogin::cancel() {
  if (!active_) {
    return;
  }
  closing_ = true;
  closeBrowser();
}

void OverframeLogin::pollDebugger() {
  if (!active_ || closing_ || discoveryPending_ ||
      webSocket_->state() != QAbstractSocket::UnconnectedState) {
    return;
  }
  discoveryPending_ = true;
  auto *reply = network_->get(QNetworkRequest(
      QUrl(QString("http://127.0.0.1:%1/json/version").arg(port_))));
  connect(reply, &QNetworkReply::finished, this, [this, reply] {
    discoveryPending_ = false;
    if (!active_ || closing_) {
      reply->deleteLater();
      return;
    }
    const QJsonDocument document = QJsonDocument::fromJson(reply->readAll());
    const QString debuggerUrl =
        document.object().value("webSocketDebuggerUrl").toString();
    reply->deleteLater();
    if (!debuggerUrl.isEmpty()) {
      connectDebugger(debuggerUrl);
    }
  });
}

void OverframeLogin::connectDebugger(const QString &url) {
  if (webSocket_->state() == QAbstractSocket::UnconnectedState) {
    webSocket_->open(QUrl(url));
  }
}

void OverframeLogin::requestCookies() {
  if (!active_ || closing_ || !webSocket_->isValid() || cookieCommandId_ != 0) {
    return;
  }
  cookieCommandId_ = nextCommandId_++;
  const QJsonObject command{{"id", cookieCommandId_},
                            {"method", "Storage.getCookies"}};
  webSocket_->sendTextMessage(
      QString::fromUtf8(QJsonDocument(command).toJson(QJsonDocument::Compact)));
}

void OverframeLogin::handleDebuggerMessage(const QString &message) {
  const QJsonObject response =
      QJsonDocument::fromJson(message.toUtf8()).object();
  if (response.value("id").toInt() != cookieCommandId_) {
    return;
  }
  cookieCommandId_ = 0;

  QJsonArray cookies;
  bool hasSession = false;
  for (const QJsonValue &value :
       response.value("result").toObject().value("cookies").toArray()) {
    const QJsonObject cookie = value.toObject();
    const QString domain = cookie.value("domain").toString();
    if (!overframeDomain(domain)) {
      continue;
    }
    const QString name = cookie.value("name").toString();
    hasSession = hasSession || name == "sessionid";
    cookies.append(QJsonObject{{"name", name},
                               {"value", cookie.value("value")},
                               {"domain", domain},
                               {"expires", cookie.value("expires")}});
  }
  if (!hasSession) {
    return;
  }

  closing_ = true;
  cookieTimer_->stop();
  emit cookiesReady(cookies);
  closeBrowser();
}

void OverframeLogin::closeBrowser() {
  if (webSocket_->isValid()) {
    const QJsonObject command{{"id", nextCommandId_++},
                              {"method", "Browser.close"}};
    webSocket_->sendTextMessage(QString::fromUtf8(
        QJsonDocument(command).toJson(QJsonDocument::Compact)));
  } else if (process_->state() != QProcess::NotRunning) {
    process_->terminate();
  } else {
    finish();
    return;
  }
  QTimer::singleShot(2000, this, [this] {
    if (!active_) {
      return;
    }
    if (process_->state() != QProcess::NotRunning) {
      process_->terminate();
      QTimer::singleShot(1000, this, [this] {
        if (active_ && process_->state() != QProcess::NotRunning) {
          process_->kill();
        }
      });
    } else {
      finish();
    }
  });
}

void OverframeLogin::finish() {
  if (!active_) {
    return;
  }
  discoveryTimer_->stop();
  startupTimer_->stop();
  cookieTimer_->stop();
  webSocket_->abort();
  active_ = false;
  closing_ = false;
  discoveryPending_ = false;
  removeProfile();
  emit activeChanged(false);
}

void OverframeLogin::fail(const QString &error) {
  if (!active_) {
    emit failed(error);
    return;
  }
  closing_ = true;
  if (process_->state() != QProcess::NotRunning) {
    process_->terminate();
    if (!process_->waitForFinished(500)) {
      process_->kill();
      process_->waitForFinished(500);
    }
  }
  finish();
  emit failed(error);
}

void OverframeLogin::removeProfile() {
  if (profilePath_.isEmpty()) {
    return;
  }
  QDir(profilePath_).removeRecursively();
  profilePath_.clear();
}
