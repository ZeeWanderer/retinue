#include "settings_widget.h"

#include <QFrame>
#include <QHBoxLayout>
#include <QJsonObject>
#include <QLabel>
#include <QList>
#include <QLocale>
#include <QMessageBox>
#include <QMetaObject>
#include <QPointer>
#include <QPushButton>
#include <QShowEvent>
#include <QStyle>
#include <QThreadPool>
#include <QVBoxLayout>

#include "app_controller.h"
#include "image_cache.h"
#include "widget_capture.h"

namespace {
QString sizeText(qint64 bytes) {
  return QLocale().formattedDataSize(bytes, 1,
                                     QLocale::DataSizeTraditionalFormat);
}

QLabel *sectionHeading(const QString &text) {
  auto *heading = new QLabel(text);
  heading->setObjectName("settingsSectionHeading");
  return heading;
}

QWidget *cacheRow(const QString &title, const QString &description,
                  QLabel *usage, QLabel *path, QPushButton *clear,
                  QLabel *error = nullptr) {
  auto *row = new QWidget;
  row->setObjectName("settingsRow");
  auto *layout = new QHBoxLayout(row);
  layout->setContentsMargins(16, 12, 16, 12);
  layout->setSpacing(16);

  auto *content = new QVBoxLayout;
  content->setSpacing(3);

  auto *heading = new QLabel(title);
  heading->setObjectName("settingsHeading");
  content->addWidget(heading);
  auto *detail = new QLabel(description);
  detail->setObjectName("settingsDescription");
  detail->setWordWrap(true);
  content->addWidget(detail);
  usage->setObjectName("settingsValue");
  content->addWidget(usage);
  if (path) {
    path->setObjectName("settingsPath");
    path->setTextInteractionFlags(Qt::TextSelectableByMouse);
    path->setWordWrap(true);
    content->addWidget(path);
  }
  if (error) {
    content->addWidget(error);
  }
  layout->addLayout(content, 1);
  layout->addWidget(clear, 0, Qt::AlignVCenter);
  return row;
}

QWidget *daemonRow(QLabel *status) {
  auto *row = new QWidget;
  row->setObjectName("settingsRow");
  auto *layout = new QHBoxLayout(row);
  layout->setContentsMargins(16, 14, 16, 14);
  layout->setSpacing(16);

  auto *content = new QVBoxLayout;
  content->setSpacing(3);
  auto *heading = new QLabel("Retinue daemon");
  heading->setObjectName("settingsHeading");
  content->addWidget(heading);
  auto *detail = new QLabel("Background data service");
  detail->setObjectName("settingsDescription");
  content->addWidget(detail);
  layout->addLayout(content, 1);

  status->setObjectName("settingsDaemonStatus");
  status->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
  status->setWordWrap(true);
  layout->addWidget(status, 0, Qt::AlignVCenter);
  return row;
}

QWidget *settingsGroup(const QList<QWidget *> &rows) {
  auto *group = new QWidget;
  group->setObjectName("settingsGroup");
  auto *layout = new QVBoxLayout(group);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(0);
  for (int index = 0; index < rows.size(); ++index) {
    layout->addWidget(rows.at(index));
    if (index + 1 < rows.size()) {
      auto *separator = new QFrame;
      separator->setObjectName("settingsSeparator");
      separator->setFrameShape(QFrame::HLine);
      layout->addWidget(separator);
    }
  }
  return group;
}
} // namespace

SettingsWidget::SettingsWidget(AppController *controller, QWidget *parent)
    : QWidget(parent), controller_(controller), daemonStatus_(new QLabel),
      memoryUsage_(new QLabel), localUsage_(new QLabel), localPath_(new QLabel),
      sourceUsage_(new QLabel), sourcePath_(new QLabel),
      sourceError_(new QLabel), clearLocal_(new QPushButton("Clear")),
      clearSource_(new QPushButton("Clear")),
      maintenancePool_(new QThreadPool(this)) {
  setObjectName("page");
  clearLocal_->setObjectName("clearDerivativeCache");
  clearSource_->setObjectName("clearSourceCache");
  clearLocal_->setProperty("destructive", true);
  clearSource_->setProperty("destructive", true);
  maintenancePool_->setMaxThreadCount(1);

  auto *layout = new QVBoxLayout(this);
  layout->setContentsMargins(20, 18, 20, 18);
  layout->setSpacing(8);
  auto *title = new QLabel("Settings");
  title->setObjectName("pageTitle");
  layout->addWidget(title);
  layout->addSpacing(8);

  layout->addWidget(sectionHeading("Daemon"));
  auto *daemonGroup = settingsGroup({daemonRow(daemonStatus_)});
  wfgui::setCaptureTarget(daemonGroup, "settings.daemon");
  layout->addWidget(daemonGroup);
  layout->addSpacing(10);

  layout->addWidget(sectionHeading("Storage"));

  auto *clearMemory = new QPushButton("Clear");
  clearMemory->setObjectName("clearMemoryCache");
  clearMemory->setProperty("destructive", true);
  sourceError_->setObjectName("settingsError");
  sourceError_->setWordWrap(true);
  sourceError_->hide();
  auto *storageGroup = settingsGroup(
      {cacheRow("Image memory", "Render-ready images kept in memory.",
                memoryUsage_, nullptr, clearMemory),
       cacheRow("Rendered images",
                "Resolution-specific images generated from source assets.",
                localUsage_, localPath_, clearLocal_),
       cacheRow("Source assets", "Original images managed by Retinue daemon.",
                sourceUsage_, sourcePath_, clearSource_, sourceError_)});
  wfgui::setCaptureTarget(storageGroup, "settings.storage");
  layout->addWidget(storageGroup);
  layout->addStretch();

  connect(clearMemory, &QPushButton::clicked, this, [this] {
    wfgui::clearThumbnailMemoryCache();
    memoryUsage_->setText(sizeText(wfgui::thumbnailMemoryCacheLimit()) +
                          " limit; cleared");
  });
  connect(clearLocal_, &QPushButton::clicked, this,
          &SettingsWidget::clearLocalCache);
  connect(clearSource_, &QPushButton::clicked, this, [this] {
    if (QMessageBox::question(this, "Clear source assets",
                              "Remove all downloaded source assets?") ==
        QMessageBox::Yes) {
      controller_->clearSourceAssetCache();
    }
  });
  connect(controller_, &AppController::sourceAssetCacheChanged, this,
          &SettingsWidget::updateSourceCache);
  connect(controller_, &AppController::statusChanged, this,
          &SettingsWidget::updateDaemonStatus);
  connect(controller_, &AppController::connectedChanged, this,
          &SettingsWidget::updateDaemonStatus);
  updateDaemonStatus();
  updateSourceCache();
}

void SettingsWidget::showEvent(QShowEvent *event) {
  QWidget::showEvent(event);
  refresh();
}

void SettingsWidget::refresh() {
  memoryUsage_->setText(sizeText(wfgui::thumbnailMemoryCacheLimit()) +
                        " limit");
  refreshLocalCache();
  controller_->refreshSourceAssetCache();
}

void SettingsWidget::refreshLocalCache() {
  if (localBusy_) {
    return;
  }
  localBusy_ = true;
  localUsage_->setText("Calculating...");
  clearLocal_->setEnabled(false);
  QPointer<SettingsWidget> self(this);
  maintenancePool_->start([self] {
    const wfgui::DerivativeCacheStats stats = wfgui::derivativeCacheStats();
    if (!self) {
      return;
    }
    QMetaObject::invokeMethod(
        self,
        [self, stats] {
          if (!self) {
            return;
          }
          self->localBusy_ = false;
          self->localUsage_->setText(QString("%1 image%2, %3")
                                         .arg(stats.files)
                                         .arg(stats.files == 1 ? "" : "s")
                                         .arg(sizeText(stats.bytes)));
          self->localPath_->setText(stats.path);
          self->clearLocal_->setEnabled(stats.files > 0);
        },
        Qt::QueuedConnection);
  });
}

void SettingsWidget::updateDaemonStatus() {
  daemonStatus_->setText(controller_->connected() ? "Connected"
                                                  : "Disconnected");
  daemonStatus_->setToolTip(controller_->status());
  daemonStatus_->setProperty("connected", controller_->connected());
  daemonStatus_->style()->unpolish(daemonStatus_);
  daemonStatus_->style()->polish(daemonStatus_);
}

void SettingsWidget::updateSourceCache() {
  const QJsonObject status = controller_->sourceAssetCache();
  const bool busy = controller_->sourceAssetCacheBusy();
  clearSource_->setEnabled(!busy && status.value("objects").toInteger() > 0);
  sourceUsage_->setText(
      busy ? "Working..."
           : QString("%1 object%2, %3")
                 .arg(status.value("objects").toInteger())
                 .arg(status.value("objects").toInteger() == 1 ? "" : "s")
                 .arg(sizeText(status.value("bytes").toInteger())));
  sourcePath_->setText(status.value("cache_root").toString());
  sourceError_->setText(controller_->sourceAssetCacheError());
  sourceError_->setVisible(!sourceError_->text().isEmpty());
}

void SettingsWidget::clearLocalCache() {
  if (localBusy_ ||
      QMessageBox::question(this, "Clear rendered images",
                            "Remove all locally rendered images?") !=
          QMessageBox::Yes) {
    return;
  }
  localBusy_ = true;
  localUsage_->setText("Clearing...");
  clearLocal_->setEnabled(false);
  QPointer<SettingsWidget> self(this);
  maintenancePool_->start([self] {
    const bool cleared = wfgui::clearDerivativeCache();
    if (!self) {
      return;
    }
    QMetaObject::invokeMethod(
        self,
        [self, cleared] {
          if (!self) {
            return;
          }
          self->localBusy_ = false;
          if (!cleared) {
            self->localUsage_->setText("Could not clear cache");
            self->clearLocal_->setEnabled(true);
          } else {
            self->refreshLocalCache();
          }
        },
        Qt::QueuedConnection);
  });
}
