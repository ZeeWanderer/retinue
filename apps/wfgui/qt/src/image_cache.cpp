#include "image_cache.h"

#include <QApplication>
#include <QDateTime>
#include <QHash>
#include <QImageReader>
#include <QList>
#include <QMutex>
#include <QMutexLocker>
#include <QPaintDevice>
#include <QPainter>
#include <QPixmapCache>
#include <QPointer>
#include <QQueue>
#include <QRegion>
#include <QSet>
#include <QThread>
#include <QThreadPool>
#include <QTimer>
#include <QWidget>
#include <QtMath>

#include <algorithm>
#include <atomic>
#include <memory>
#include <utility>

#ifdef Q_OS_LINUX
#include <sys/resource.h>
#include <sys/syscall.h>
#include <unistd.h>
#endif

#include "derivative_cache.h"

namespace {
constexpr int MemoryCacheKiB = 64 * 1024;
constexpr int ReadyBatchSize = 4;
constexpr qint64 MaximumSourcePixels = 64 * 1024 * 1024;
constexpr int MaximumSourceDimension = 16384;
constexpr qint64 DecodeBudget = 32 * 1024 * 1024;
constexpr qint64 WriteBudget = 32 * 1024 * 1024;

int lowerWorkerPriority() {
#ifdef Q_OS_LINUX
  const auto tid = static_cast<id_t>(syscall(SYS_gettid));
  if (getpriority(PRIO_PROCESS, tid) < 5) {
    setpriority(PRIO_PROCESS, tid, 5);
  }
  return getpriority(PRIO_PROCESS, tid);
#else
  return 0;
#endif
}

wfgui::ImageIssueReporter imageIssueReporter;

struct DecodedImage {
  QImage image;
  QString error;
};

bool oversized(const QSize &size) {
  return size.isValid() && (size.width() > MaximumSourceDimension ||
                            size.height() > MaximumSourceDimension ||
                            static_cast<qint64>(size.width()) * size.height() >
                                MaximumSourcePixels);
}

QImage normalized(QImage image) {
  if (!image.isNull() && image.format() != QImage::Format_RGB32 &&
      image.format() != QImage::Format_ARGB32_Premultiplied) {
    return image.convertToFormat(QImage::Format_ARGB32_Premultiplied);
  }
  return image;
}

DecodedImage decodeThumbnail(const QString &path, const QSize &pixelBounds) {
  if (path.isEmpty() || pixelBounds.isEmpty()) {
    return {{}, "invalid image path or bounds"};
  }
  QImageReader reader(path);
  reader.setAutoTransform(true);
  const QSize source = reader.size();
  if (oversized(source)) {
    return {{}, "image dimensions exceed safety limit"};
  }

  QSize targetSize;
  if (source.isValid()) {
    targetSize = source.scaled(pixelBounds, Qt::KeepAspectRatio);
    targetSize.setWidth(std::max(1, targetSize.width()));
    targetSize.setHeight(std::max(1, targetSize.height()));
    if (targetSize != source) {
      reader.setScaledSize(targetSize);
    }
  }

  QImage decoded = reader.read();
  if (decoded.isNull() || oversized(decoded.size())) {
    return {{}, decoded.isNull() ? reader.errorString()
                                 : QString("image dimensions exceed safety limit")};
  }
  if (!targetSize.isValid()) {
    targetSize = decoded.size().scaled(pixelBounds, Qt::KeepAspectRatio);
    targetSize.setWidth(std::max(1, targetSize.width()));
    targetSize.setHeight(std::max(1, targetSize.height()));
  }
  decoded = normalized(std::move(decoded));
  if (decoded.size() != targetSize) {
    decoded = decoded.scaled(targetSize, Qt::IgnoreAspectRatio,
                             Qt::SmoothTransformation);
  }
  return {std::move(decoded), {}};
}

QPixmap toPixmap(QImage image, qreal dpr) {
  if (image.isNull()) {
    return {};
  }
  QPixmap pixmap = QPixmap::fromImage(std::move(image));
  pixmap.setDevicePixelRatio(dpr);
  return pixmap;
}

wfgui::DerivativeCache &derivativeCache() {
  static wfgui::DerivativeCache cache;
  return cache;
}

class ThumbnailLoader final : public QObject {
public:
  explicit ThumbnailLoader(QObject *parent) : QObject(parent) {
    const int cores = std::max(1, QThread::idealThreadCount());
    pool_.setMaxThreadCount(std::clamp(cores - 1, 1, 3));
    pool_.setThreadPriority(QThread::LowPriority);
    writer_.setMaxThreadCount(1);
    writer_.setThreadPriority(QThread::LowestPriority);
    QPixmapCache::setCacheLimit(
        std::max(QPixmapCache::cacheLimit(), MemoryCacheKiB));
    connect(QApplication::instance(), &QCoreApplication::aboutToQuit, this,
            [this] { stop(); });
  }

  ~ThumbnailLoader() override {
    stop();
    pool_.waitForDone();
    writer_.waitForDone();
  }

  void acceptAsset(const wfgui::AssetRef &asset) {
    if (!asset.isPersistent() || stopping_.load()) {
      return;
    }
    QPointer<ThumbnailLoader> loader(this);
    // One FIFO writer orders source changes before lower-priority derivative writes.
    writer_.start([loader, asset] {
      lowerWorkerPriority();
      if (loader && !loader->stopping_.load()) {
        derivativeCache().registerAsset(asset);
      }
    });
  }

  void request(const QString &key, const wfgui::AssetRef &asset,
               const QSize &pixelBounds, qreal dpr, QWidget *target,
               const QRect &dirtyRegion) {
    const qint64 now = QDateTime::currentMSecsSinceEpoch();
    if (stopping_.load()) {
      return;
    }
    const auto failure = failures_.constFind(key);
    if (failure != failures_.cend() && failure->retryAt > now) {
      return;
    }

    auto &waiters = waiters_[key];
    auto waiter = std::find_if(
        waiters.begin(), waiters.end(),
        [target](const Waiter &item) { return item.target == target; });
    const bool fullUpdate = !dirtyRegion.isValid() || dirtyRegion.isEmpty();
    if (waiter == waiters.end()) {
      waiters.append(Waiter{
          target, fullUpdate ? QRegion{} : QRegion(dirtyRegion), fullUpdate});
    } else if (fullUpdate) {
      waiter->region = {};
      waiter->full = true;
    } else if (!waiter->full) {
      waiter->region += dirtyRegion;
    }
    if (active_.contains(key)) {
      return;
    }
    queued_.insert(key, Request{asset, pixelBounds, dpr});
    queue_.removeAll(key);
    queue_.append(key);
    dispatch();
  }

  void clearMemory() {
    QPixmapCache::clear();
    failures_.clear();
  }

  wfgui::ThumbnailWorkStats stats() {
    const QMutexLocker lock(&readyMutex_);
    qint64 readyBytes = 0;
    for (const ReadyResult &result : ready_) {
      readyBytes += result.image.sizeInBytes();
    }
    return {.queued = static_cast<int>(queue_.size()),
            .active = static_cast<int>(active_.size()),
            .ready = static_cast<int>(ready_.size()),
            .readyBytes = readyBytes,
            .reservedBytes = reservedBytes_,
            .writeBytes = writeBytes_.load(),
            .skippedWrites = skippedWrites_.load(),
            .workerNice = workerNice_.load()};
  }

private:
  struct Request {
    wfgui::AssetRef asset;
    QSize pixelBounds;
    qreal dpr;
    qint64 bytes() const {
      return static_cast<qint64>(pixelBounds.width()) * pixelBounds.height() *
             4;
    }
  };

  struct ReadyResult {
    QString key;
    wfgui::AssetRef asset;
    QImage image;
    QString error;
    qreal dpr;
  };

  struct Failure {
    int attempts = 0;
    qint64 retryAt = 0;
  };

  struct Waiter {
    QPointer<QWidget> target;
    QRegion region;
    bool full = false;
  };

  struct TargetUpdate {
    QRegion region;
    bool full = false;
  };

  void stop() {
    if (stopping_.exchange(true)) {
      return;
    }
    pool_.clear();
    writer_.clear();
    queue_.clear();
    queued_.clear();
    active_.clear();
    reservedBytes_ = 0;
    waiters_.clear();
    const QMutexLocker lock(&readyMutex_);
    ready_.clear();
    drainScheduled_ = false;
  }

  void dispatch() {
    while (!stopping_.load() && !queue_.isEmpty() &&
           active_.size() < pool_.maxThreadCount()) {
      const QString key = queue_.last();
      const auto &waiters = waiters_[key];
      const bool requested = std::any_of(
          waiters.cbegin(), waiters.cend(),
          [](const Waiter &waiter) { return !waiter.target.isNull(); });
      if (!requested) {
        queue_.removeLast();
        queued_.remove(key);
        waiters_.remove(key);
        continue;
      }
      const Request request = queued_.value(key);
      // A larger preview can run alone; ordinary thumbnails share the byte
      // budget.
      if (!active_.isEmpty() &&
          reservedBytes_ + request.bytes() > DecodeBudget) {
        break;
      }
      queue_.removeLast();
      queued_.remove(key);
      active_.insert(key, request.bytes());
      reservedBytes_ += request.bytes();
      pool_.start([this, key, request] {
        workerNice_.store(lowerWorkerPriority());
        if (stopping_.load()) {
          return;
        }
        QImage image =
            derivativeCache().load(request.asset, request.pixelBounds);
        const bool needsStore = image.isNull() && request.asset.isPersistent();
        QString error;
        if (image.isNull()) {
          DecodedImage decoded =
              decodeThumbnail(request.asset.path, request.pixelBounds);
          image = std::move(decoded.image);
          error = std::move(decoded.error);
        } else {
          image = normalized(std::move(image));
        }
        if (needsStore && !image.isNull() && !stopping_.load()) {
          storeLater(request.asset, request.pixelBounds, image);
        }
        enqueueResult(key, request.asset, std::move(image), std::move(error),
                      request.dpr);
      });
    }
  }

  void storeLater(const wfgui::AssetRef &asset, const QSize &pixelBounds,
                  const QImage &image) {
    const qint64 bytes = image.sizeInBytes();
    qint64 reserved = writeBytes_.load();
    do {
      if (reserved + bytes > WriteBudget) {
        ++skippedWrites_;
        return;
      }
    } while (!writeBytes_.compare_exchange_weak(reserved, reserved + bytes));
    // The reservation also releases if shutdown discards a queued write.
    const std::shared_ptr<QImage> pending(new QImage(image),
                                          [this, bytes](QImage *value) {
                                            delete value;
                                            writeBytes_.fetch_sub(bytes);
                                          });
    writer_.start(
        [this, asset, pixelBounds, pending] {
          lowerWorkerPriority();
          if (!stopping_.load()) {
            (void)derivativeCache().store(asset, pixelBounds, *pending);
          }
        },
        -1);
  }

  void enqueueResult(QString key, wfgui::AssetRef asset, QImage image,
                     QString error, qreal dpr) {
    bool schedule = false;
    {
      const QMutexLocker lock(&readyMutex_);
      if (stopping_.load()) {
        return;
      }
      ready_.enqueue(ReadyResult{std::move(key), std::move(asset),
                                 std::move(image), std::move(error), dpr});
      if (!drainScheduled_) {
        drainScheduled_ = true;
        schedule = true;
      }
    }
    if (schedule) {
      QMetaObject::invokeMethod(
          this, [this] { drainResults(); }, Qt::QueuedConnection);
    }
  }

  void drainResults() {
    QQueue<ReadyResult> batch;
    bool more = false;
    {
      const QMutexLocker lock(&readyMutex_);
      for (int count = 0; count < ReadyBatchSize && !ready_.isEmpty();
           ++count) {
        batch.enqueue(ready_.dequeue());
      }
      more = !ready_.isEmpty();
      if (!more) {
        drainScheduled_ = false;
      }
    }

    QHash<QWidget *, TargetUpdate> targets;
    while (!batch.isEmpty()) {
      ReadyResult result = batch.dequeue();
      finish(result.key, result.asset, std::move(result.image), result.error,
             result.dpr, targets);
    }
    for (auto target = targets.cbegin(); target != targets.cend(); ++target) {
      if (target.value().full) {
        target.key()->update();
      } else {
        target.key()->update(target.value().region);
      }
    }
    dispatch();
    if (more) {
      QTimer::singleShot(0, this, [this] { drainResults(); });
    }
  }

  void finish(const QString &key, const wfgui::AssetRef &asset, QImage image,
              const QString &error, qreal dpr,
              QHash<QWidget *, TargetUpdate> &targets) {
    reservedBytes_ -= active_.take(key);
    if (image.isNull()) {
      Failure &failure = failures_[key];
      failure.attempts = std::min(failure.attempts + 1, 6);
      const qint64 delay = std::min<qint64>(60'000, 1000LL << failure.attempts);
      failure.retryAt = QDateTime::currentMSecsSinceEpoch() + delay;
      if (imageIssueReporter) {
        imageIssueReporter(asset, error.isEmpty() ? "image decode failed" : error,
                           false);
      }
    } else {
      const bool wasFailed = failures_.contains(key);
      failures_.remove(key);
      QPixmapCache::insert(key, toPixmap(std::move(image), dpr));
      if (wasFailed && imageIssueReporter) {
        imageIssueReporter(asset, {}, true);
      }
    }
    const auto waiters = waiters_.take(key);
    for (const Waiter &waiter : waiters) {
      if (waiter.target) {
        TargetUpdate &update = targets[waiter.target.data()];
        if (waiter.full) {
          update.region = {};
          update.full = true;
        } else if (!update.full) {
          update.region += waiter.region;
        }
      }
    }
  }

  QThreadPool pool_;
  QThreadPool writer_;
  QList<QString> queue_;
  QHash<QString, Request> queued_;
  QHash<QString, qint64> active_;
  QHash<QString, Failure> failures_;
  QHash<QString, QList<Waiter>> waiters_;
  QMutex readyMutex_;
  QQueue<ReadyResult> ready_;
  std::atomic_bool stopping_ = false;
  std::atomic<qint64> writeBytes_ = 0;
  std::atomic<qint64> skippedWrites_ = 0;
  std::atomic_int workerNice_ = 0;
  qint64 reservedBytes_ = 0;
  bool drainScheduled_ = false;
};

ThumbnailLoader *thumbnailLoader() {
  static QPointer<ThumbnailLoader> loader;
  if (!loader) {
    loader = new ThumbnailLoader(QApplication::instance());
  }
  return loader;
}

QString memoryKey(const wfgui::AssetRef &asset, const QSize &pixelBounds,
                  qreal dpr) {
  const QString identity = asset.digest.isEmpty() ? asset.path : asset.digest;
  return QString("wfgui-thumb:contain-v1:%1:%2x%3@%4")
      .arg(identity)
      .arg(pixelBounds.width())
      .arg(pixelBounds.height())
      .arg(dpr, 0, 'g', 12);
}
} // namespace

namespace wfgui {

QPixmap cachedThumbnail(QPainter &painter, const QString &path,
                        const QSize &logicalBounds, QRect dirtyRegion) {
  return cachedThumbnail(painter, AssetRef::embedded(path, path), logicalBounds,
                         dirtyRegion);
}

QPixmap cachedThumbnail(QPainter &painter, const AssetRef &asset,
                        const QSize &logicalBounds, QRect dirtyRegion) {
  QPixmap image;
  if (!asset.isValid() || logicalBounds.isEmpty()) {
    return image;
  }

  const qreal dpr =
      painter.device() ? painter.device()->devicePixelRatioF() : 1.0;
  const QSize pixelBounds(qCeil(logicalBounds.width() * dpr),
                          qCeil(logicalBounds.height() * dpr));
  if (oversized(pixelBounds)) {
    return {};
  }
  const QString key = memoryKey(asset, pixelBounds, dpr);
  if (QPixmapCache::find(key, &image)) {
    return image;
  }

  QWidget *target = dynamic_cast<QWidget *>(painter.device());
  if (target && !asset.path.startsWith(":/")) {
    ThumbnailLoader *loader = thumbnailLoader();
    loader->request(key, asset, pixelBounds, dpr, target, dirtyRegion);
    return {};
  }

  DecodedImage decoded = decodeThumbnail(asset.path, pixelBounds);
  image = toPixmap(std::move(decoded.image), dpr);
  if (!image.isNull()) {
    QPixmapCache::insert(key, image);
  } else if (imageIssueReporter) {
    imageIssueReporter(asset,
                       decoded.error.isEmpty() ? "image decode failed"
                                               : decoded.error,
                       false);
  }
  return image;
}

void acceptThumbnailAsset(const AssetRef &asset) {
  thumbnailLoader()->acceptAsset(asset);
}

DerivativeCacheStats derivativeCacheStats() {
  return derivativeCache().stats();
}

bool clearDerivativeCache() { return derivativeCache().clear(); }

void clearThumbnailMemoryCache() { thumbnailLoader()->clearMemory(); }

qint64 thumbnailMemoryCacheLimit() {
  return static_cast<qint64>(QPixmapCache::cacheLimit()) * 1024;
}

ThumbnailWorkStats thumbnailWorkStats() { return thumbnailLoader()->stats(); }

void setImageIssueReporter(ImageIssueReporter reporter) {
  imageIssueReporter = std::move(reporter);
}

void drawContained(QPainter &painter, const QRectF &rect,
                   const QPixmap &image) {
  if (image.isNull()) {
    return;
  }
  QSizeF size = image.deviceIndependentSize();
  const qreal dpr = image.devicePixelRatio();
  const qreal tolerance = dpr > 0.0 ? 1.0 / dpr : 1.0;
  if (size.width() > rect.width() + tolerance ||
      size.height() > rect.height() + tolerance) {
    size.scale(rect.size(), Qt::KeepAspectRatio);
    const QRectF target(QPointF(rect.center().x() - size.width() / 2.0,
                                rect.center().y() - size.height() / 2.0),
                        size);
    painter.drawPixmap(target, image, QRectF(image.rect()));
    return;
  }

  QPointF topLeft(rect.center().x() - size.width() / 2.0,
                  rect.center().y() - size.height() / 2.0);
  const qreal paintDpr =
      painter.device() ? painter.device()->devicePixelRatioF() : 1.0;
  if (paintDpr > 0.0) {
    topLeft.setX(qRound(topLeft.x() * paintDpr) / paintDpr);
    topLeft.setY(qRound(topLeft.y() * paintDpr) / paintDpr);
  }
  painter.save();
  painter.setClipRect(rect);
  painter.drawPixmap(topLeft, image);
  painter.restore();
}

} // namespace wfgui
