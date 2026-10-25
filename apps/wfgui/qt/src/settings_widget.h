#pragma once

#include <QWidget>

class AppController;
class QLabel;
class QPushButton;
class QShowEvent;
class QThreadPool;

class SettingsWidget final : public QWidget {
  Q_OBJECT

public:
  explicit SettingsWidget(AppController *controller, QWidget *parent = nullptr);

protected:
  void showEvent(QShowEvent *event) override;

private:
  void refresh();
  void refreshLocalCache();
  void updateDaemonStatus();
  void updateSourceCache();
  void clearLocalCache();

  AppController *controller_;
  QLabel *daemonStatus_;
  QLabel *memoryUsage_;
  QLabel *localUsage_;
  QLabel *localPath_;
  QLabel *sourceUsage_;
  QLabel *sourcePath_;
  QLabel *sourceError_;
  QPushButton *clearLocal_;
  QPushButton *clearSource_;
  QThreadPool *maintenancePool_;
  bool localBusy_ = false;
};
