#include "animated_progress_bar.h"

#include <QHideEvent>
#include <QPainter>
#include <QShowEvent>
#include <QTimer>

#include <algorithm>

namespace {
constexpr int AnimationDurationMs = 1100;
constexpr int FrameIntervalMs = 16;
} // namespace

AnimatedProgressBar::AnimatedProgressBar(QWidget *parent)
    : QProgressBar(parent), timer_(new QTimer(this)) {
  timer_->setInterval(FrameIntervalMs);
  connect(timer_, &QTimer::timeout, this,
          qOverload<>(&AnimatedProgressBar::update));
}

void AnimatedProgressBar::setRange(int minimum, int maximum) {
  QProgressBar::setRange(minimum, maximum);
  updateAnimation();
}

void AnimatedProgressBar::showEvent(QShowEvent *event) {
  QProgressBar::showEvent(event);
  updateAnimation();
}

void AnimatedProgressBar::hideEvent(QHideEvent *event) {
  timer_->stop();
  QProgressBar::hideEvent(event);
}

void AnimatedProgressBar::paintEvent(QPaintEvent *event) {
  if (minimum() != 0 || maximum() != 0) {
    QProgressBar::paintEvent(event);
    return;
  }

  Q_UNUSED(event);
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing, height() > 2);

  const bool thin = objectName() == QStringLiteral("priceProgress");
  QRectF track = rect();
  if (!thin) {
    track.adjust(0.5, 0.5, -0.5, -0.5);
    painter.setPen(QColor("#313a58"));
    painter.setBrush(QColor("#20283e"));
    painter.drawRoundedRect(track, 3, 3);
    track.adjust(1, 1, -1, -1);
  }

  painter.setClipRect(track);
  const qreal segmentWidth = std::max<qreal>(24, track.width() * 0.22);
  const qreal phase = elapsed_.isValid()
                          ? (elapsed_.elapsed() % AnimationDurationMs) /
                                static_cast<qreal>(AnimationDurationMs)
                          : 0;
  const qreal x =
      track.left() - segmentWidth + phase * (track.width() + segmentWidth);
  painter.setPen(Qt::NoPen);
  painter.setBrush(QColor("#5e44af"));
  painter.drawRoundedRect(QRectF(x, track.top(), segmentWidth, track.height()),
                          thin ? 1 : 3, thin ? 1 : 3);
}

void AnimatedProgressBar::updateAnimation() {
  const bool busy = minimum() == 0 && maximum() == 0;
  if (!busy || !isVisible()) {
    timer_->stop();
    return;
  }
  if (!elapsed_.isValid()) {
    elapsed_.start();
  }
  if (!timer_->isActive()) {
    timer_->start();
  }
  update();
}
