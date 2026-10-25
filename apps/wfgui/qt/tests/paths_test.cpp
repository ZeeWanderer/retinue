#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSettings>
#include <QTemporaryDir>
#include <QtTest>

#include <sys/stat.h>
#include <unistd.h>

#include "display_scale.h"
#include "wfgui_paths.h"

namespace {
class Environment {
public:
  Environment() {
    for (const char *name : {"HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME",
                             "XDG_RUNTIME_DIR", "TMPDIR",
                             "RETINUE_DAEMON_SOCKET", "WFCLI_DAEMON_SOCKET"}) {
      saved_.append({name, qgetenv(name)});
      qunsetenv(name);
    }
  }

  ~Environment() {
    for (const auto &[name, value] : saved_) {
      if (value.isNull()) {
        qunsetenv(name.constData());
      } else {
        qputenv(name.constData(), value);
      }
    }
  }

private:
  QList<QPair<QByteArray, QByteArray>> saved_;
};
} // namespace

class PathsTest final : public QObject {
  Q_OBJECT

private slots:
  void absoluteXdgPathsAreGameScopedAndLazy() {
    const Environment environment;
    QTemporaryDir directory;
    QVERIFY(directory.isValid());
    qputenv("HOME", "/unused");
    qputenv("XDG_CONFIG_HOME", directory.filePath("config").toUtf8());
    qputenv("XDG_CACHE_HOME", directory.filePath("cache").toUtf8());
    qputenv("XDG_RUNTIME_DIR", directory.filePath("runtime").toUtf8());

    QCOMPARE(wfgui::settingsFilePath(),
             directory.filePath("config/retinue/games/warframe/gui.conf"));
    QCOMPARE(wfgui::cacheDirectory(),
             directory.filePath("cache/retinue/games/warframe/gui"));
    QCOMPARE(wfgui::derivativeCacheDirectory(),
             directory.filePath("cache/retinue/games/warframe/gui/derivatives/v1"));
    QCOMPARE(wfgui::daemonSocketPath(),
             directory.filePath("runtime/retinue/daemon.sock"));
    const auto report = QJsonDocument::fromJson(wfgui::pathReportJson()).object();
    QCOMPARE(report.value("app").toString(), QString("wfgui"));
    QMap<QString, QString> paths;
    for (const auto &value : report.value("paths").toArray()) {
      const auto entry = value.toObject();
      paths.insert(entry.value("kind").toString(), entry.value("path").toString());
    }
    QCOMPARE(paths.value("config"),
             directory.filePath("config/retinue/games/warframe"));
    QCOMPARE(paths.value("cache"), wfgui::cacheDirectory());
    QCOMPARE(paths.value("derivatives"), wfgui::derivativeCacheDirectory());
    QCOMPARE(paths.value("runtime"), directory.filePath("runtime/retinue"));
    QCOMPARE(QDir(directory.path()).entryList(QDir::NoDotAndDotDot | QDir::AllEntries).size(), 0);
  }

  void invalidXdgPathsUseHomeAndUidFallbacks() {
    const Environment environment;
    qputenv("HOME", "/home/test");
    qputenv("TMPDIR", "/unused/remote");
    const QString socket = QString("/tmp/retinue-%1/daemon.sock").arg(getuid());
    for (const auto &value : {QByteArray(), QByteArray(""), QByteArray("relative"),
                              QByteArray("~/cache")}) {
      for (const char *name : {"XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR"}) {
        if (value.isNull()) {
          qunsetenv(name);
        } else {
          qputenv(name, value);
        }
      }
      QCOMPARE(wfgui::settingsFilePath(),
               QString("/home/test/.config/retinue/games/warframe/gui.conf"));
      QCOMPARE(wfgui::cacheDirectory(),
               QString("/home/test/.cache/retinue/games/warframe/gui"));
      QCOMPARE(wfgui::daemonSocketPath(), socket);
    }
    qunsetenv("HOME");
    QCOMPARE(wfgui::daemonSocketPath(), socket);
  }

  void onlyRetinueSocketOverrideIsUsed() {
    const Environment environment;
    qputenv("XDG_RUNTIME_DIR", "/runtime");
    qputenv("WFCLI_DAEMON_SOCKET", "/legacy.sock");
    QCOMPARE(wfgui::daemonSocketPath(), QString("/runtime/retinue/daemon.sock"));
    for (const char *path : {"/custom.sock", "relative.sock"}) {
      qputenv("RETINUE_DAEMON_SOCKET", path);
      QCOMPARE(wfgui::daemonSocketPath(), QString::fromUtf8(path));
    }
    qputenv("RETINUE_DAEMON_SOCKET", "");
    QCOMPARE(wfgui::daemonSocketPath(), QString("/runtime/retinue/daemon.sock"));
  }

  void scaleSettingsUseExplicitFile() {
    const Environment environment;
    QTemporaryDir directory;
    QVERIFY(directory.isValid());
    qputenv("HOME", directory.path().toUtf8());
    qputenv("XDG_CONFIG_HOME", directory.path().toUtf8());
    QSettings legacy(directory.filePath("wfcli/wfgui.conf"), QSettings::IniFormat);
    legacy.setValue("ui/scale_percent", 175);
    legacy.sync();
    QCOMPARE(legacy.status(), QSettings::NoError);
    QCOMPARE(wfgui::configuredUiScalePercent(), 100);
    QVERIFY(!QFileInfo::exists(wfgui::settingsFilePath()));

    wfgui::setConfiguredUiScalePercent(128);
    QCOMPARE(wfgui::configuredUiScalePercent(), 130);
    QSettings settings(wfgui::settingsFilePath(), QSettings::IniFormat);
    QCOMPARE(settings.value("ui/scale_percent").toInt(), 130);
    QCOMPARE(legacy.value("ui/scale_percent").toInt(), 175);
    struct stat metadata {};
    QCOMPARE(::stat(QFile::encodeName(directory.filePath("retinue")).constData(), &metadata), 0);
    QCOMPARE(metadata.st_mode & 0777, mode_t(0700));
  }

  void persistentRootsKeepLogicalSymlinks() {
    const Environment environment;
    QTemporaryDir directory;
    QTemporaryDir target;
    QVERIFY(directory.isValid());
    QVERIFY(target.isValid());
    const QString logicalRoot = directory.filePath("retinue");
    QVERIFY(QFile::link(target.path(), logicalRoot));
    qputenv("XDG_CONFIG_HOME", directory.path().toUtf8());
    qputenv("XDG_CACHE_HOME", directory.path().toUtf8());
    QCOMPARE(wfgui::settingsFilePath(), QDir(logicalRoot).filePath("games/warframe/gui.conf"));
    QVERIFY(wfgui::ensurePrivateDirectory(wfgui::cacheDirectory()));
    wfgui::setConfiguredUiScalePercent(125);
    QVERIFY(QFileInfo(logicalRoot).isSymbolicLink());
    QVERIFY(QFileInfo::exists(target.filePath("games/warframe/gui.conf")));
    QVERIFY(QFileInfo(target.filePath("games/warframe/gui")).isDir());
    QVERIFY(wfgui::pathReportJson().contains(logicalRoot.toUtf8()));
    QVERIFY(!wfgui::pathReportJson().contains(target.path().toUtf8()));
    QFile file(target.filePath("occupied"));
    QVERIFY(file.open(QIODevice::WriteOnly));
    file.close();
    QVERIFY(!wfgui::ensurePrivateDirectory(file.fileName()));
    QVERIFY(QFileInfo(file.fileName()).isFile());
  }
};

QTEST_GUILESS_MAIN(PathsTest)
#include "paths_test.moc"
