#include "derivative_cache.h"
#include "wfgui_paths.h"

#include <QCryptographicHash>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QImageReader>
#include <QMutexLocker>
#include <QSaveFile>

#include <utility>

namespace {
constexpr auto Transform = "contain-v1";

QString hash(const QByteArray &value) {
  return QString::fromLatin1(
      QCryptographicHash::hash(value, QCryptographicHash::Sha256).toHex());
}

bool validBounds(const QSize &bounds) {
  constexpr int MaximumDimension = 8192;
  return bounds.width() > 0 && bounds.height() > 0 &&
         bounds.width() <= MaximumDimension &&
         bounds.height() <= MaximumDimension;
}
} // namespace

namespace wfgui {

DerivativeCache::DerivativeCache(QString root)
    : root_(root.isEmpty() ? derivativeCacheDirectory() : std::move(root)) {}

QString DerivativeCache::root() const { return root_; }

void DerivativeCache::registerAsset(const AssetRef &asset) {
  if (!asset.isPersistent()) {
    return;
  }

  const QMutexLocker lock(&mutex_);
  const QString origin = originDirectory(asset);
  const QString currentPath = QDir(origin).filePath("current");
  const QString digest = digestKey(asset);
  QFile current(currentPath);
  if (current.open(QIODevice::ReadOnly) &&
      QString::fromLatin1(current.readAll()) == digest) {
    return;
  }

  QDir().mkpath(digestDirectory(asset));
  const QDir originDir(origin);
  for (const QFileInfo &entry :
       originDir.entryInfoList(QDir::Dirs | QDir::NoDotAndDotDot)) {
    if (entry.fileName() != digest) {
      QDir(entry.absoluteFilePath()).removeRecursively();
    }
  }

  QSaveFile marker(currentPath);
  if (marker.open(QIODevice::WriteOnly)) {
    marker.write(digest.toLatin1());
    marker.commit();
  }
}

QImage DerivativeCache::load(const AssetRef &asset,
                             const QSize &pixelBounds) const {
  if (!asset.isPersistent() || !validBounds(pixelBounds)) {
    return {};
  }
  const QString path = derivativePath(asset, pixelBounds);
  if (!QFileInfo::exists(path)) {
    return {};
  }

  QImageReader reader(path);
  QImage image = reader.read();
  if (!image.isNull()) {
    return image;
  }

  const QMutexLocker lock(&mutex_);
  if (isCurrent(asset)) {
    QFile::remove(path);
  }
  return {};
}

bool DerivativeCache::store(const AssetRef &asset, const QSize &pixelBounds,
                            const QImage &image) {
  if (!asset.isPersistent() || !validBounds(pixelBounds) || image.isNull()) {
    return false;
  }

  const QMutexLocker lock(&mutex_);
  if (!isCurrent(asset)) {
    return false;
  }
  const QString path = derivativePath(asset, pixelBounds);
  if (!QDir().mkpath(QFileInfo(path).absolutePath())) {
    return false;
  }
  QSaveFile file(path);
  return file.open(QIODevice::WriteOnly) && image.save(&file, "PNG", 6) &&
         file.commit();
}

DerivativeCacheStats DerivativeCache::stats() const {
  const QMutexLocker lock(&mutex_);
  DerivativeCacheStats result{.path = root_};
  QDirIterator files(root_, {"*.png"}, QDir::Files,
                     QDirIterator::Subdirectories);
  while (files.hasNext()) {
    files.next();
    ++result.files;
    result.bytes += files.fileInfo().size();
  }
  return result;
}

bool DerivativeCache::clear() {
  const QMutexLocker lock(&mutex_);
  // Keep source markers: pending jobs must not revive superseded identities.
  QDirIterator files(root_, {"*.png"}, QDir::Files,
                     QDirIterator::Subdirectories);
  bool success = true;
  while (files.hasNext()) {
    success = QFile::remove(files.next()) && success;
  }
  return success;
}

QString DerivativeCache::originDirectory(const AssetRef &asset) const {
  QByteArray identity = asset.source.toUtf8();
  identity.append('\0');
  identity.append(asset.imageName.toUtf8());
  return QDir(root_).filePath(hash(identity));
}

QString DerivativeCache::digestDirectory(const AssetRef &asset) const {
  return QDir(originDirectory(asset)).filePath(digestKey(asset));
}

QString DerivativeCache::derivativePath(const AssetRef &asset,
                                        const QSize &pixelBounds) const {
  return QDir(digestDirectory(asset))
      .filePath(QString("%1-%2x%3.png")
                    .arg(Transform)
                    .arg(pixelBounds.width())
                    .arg(pixelBounds.height()));
}

QString DerivativeCache::digestKey(const AssetRef &asset) const {
  return hash(asset.digest.toUtf8());
}

bool DerivativeCache::isCurrent(const AssetRef &asset) const {
  QFile current(QDir(originDirectory(asset)).filePath("current"));
  return current.open(QIODevice::ReadOnly) &&
         QString::fromLatin1(current.readAll()) == digestKey(asset);
}

} // namespace wfgui
