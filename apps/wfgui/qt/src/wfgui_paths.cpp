#include "wfgui_paths.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

#include <cerrno>
#include <sys/stat.h>
#include <unistd.h>

namespace {
QString gameDirectory(const char *variable, const char *fallback) {
  QString base = qEnvironmentVariable(variable);
  if (base.isEmpty() || !QDir::isAbsolutePath(base)) {
    const QString home = qEnvironmentVariable("HOME");
    base = QDir(home.isEmpty() ? QDir::homePath() : home).filePath(fallback);
  }
  return QDir(base).filePath("retinue/games/warframe");
}

QJsonObject pathEntry(const char *kind, const QString &path) {
  return {{"kind", kind}, {"path", QFileInfo(path).absoluteFilePath()}};
}
} // namespace

namespace wfgui {

QString settingsFilePath() {
  return QDir(gameDirectory("XDG_CONFIG_HOME", ".config")).filePath("gui.conf");
}

QString cacheDirectory() {
  return QDir(gameDirectory("XDG_CACHE_HOME", ".cache")).filePath("gui");
}

QString derivativeCacheDirectory() {
  return QDir(cacheDirectory()).filePath("derivatives/v1");
}

QString daemonSocketPath() {
  const QString configured = qEnvironmentVariable("RETINUE_DAEMON_SOCKET");
  if (!configured.isEmpty()) {
    return configured;
  }
  const QString runtime = qEnvironmentVariable("XDG_RUNTIME_DIR");
  if (!runtime.isEmpty() && QDir::isAbsolutePath(runtime)) {
    return QDir(runtime).filePath("retinue/daemon.sock");
  }
  return QString("/tmp/retinue-%1/daemon.sock").arg(getuid());
}

bool ensurePrivateDirectory(const QString &path) {
  const QFileInfo info(path);
  if (info.isDir()) {
    return true;
  }
  const QString parent = info.path();
  if (parent == path || !ensurePrivateDirectory(parent)) {
    return false;
  }
  return ::mkdir(QFile::encodeName(path).constData(), 0700) == 0 ||
         (errno == EEXIST && QFileInfo(path).isDir());
}

QByteArray pathReportJson() {
  const QJsonArray paths{
      pathEntry("config", QFileInfo(settingsFilePath()).absolutePath()),
      pathEntry("cache", cacheDirectory()),
      pathEntry("derivatives", derivativeCacheDirectory()),
      pathEntry("runtime", QFileInfo(daemonSocketPath()).absolutePath()),
  };
  return QJsonDocument(QJsonObject{{"app", "wfgui"}, {"paths", paths}})
      .toJson(QJsonDocument::Compact);
}

} // namespace wfgui
