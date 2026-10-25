#include "market_spin_box.h"

#include <QPainter>
#include <QPainterPath>
#include <QStyleOptionSpinBox>

#include <algorithm>
#include <tuple>

namespace {

void drawChevron(QPainter &painter, const QRect &bounds, bool up,
                 const QColor &color) {
  const QRectF rect = bounds;
  const qreal width = std::clamp(rect.width() - 8.0, 5.0, 8.0);
  const qreal height = std::clamp(rect.height() - 8.0, 2.5, 4.0);
  const QPointF center = rect.center();
  const qreal direction = up ? -1.0 : 1.0;
  QPainterPath path;
  path.moveTo(center.x() - width / 2.0, center.y() - direction * height / 2.0);
  path.lineTo(center.x(), center.y() + direction * height / 2.0);
  path.lineTo(center.x() + width / 2.0, center.y() - direction * height / 2.0);

  painter.setPen(QPen(color, 1.8, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
  painter.setBrush(Qt::NoBrush);
  painter.drawPath(path);
}

} // namespace

namespace wfgui {

MarketSpinBox::MarketSpinBox(QWidget *parent) : QSpinBox(parent) {}

void MarketSpinBox::paintEvent(QPaintEvent *event) {
  QSpinBox::paintEvent(event);

  QStyleOptionSpinBox option;
  initStyleOption(&option);
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  for (const auto [control, step, up] : {
           std::tuple{QStyle::SC_SpinBoxUp, StepUpEnabled, true},
           std::tuple{QStyle::SC_SpinBoxDown, StepDownEnabled, false},
       }) {
    QColor color = option.stepEnabled.testFlag(step) ? QColor("#d7def2")
                                                     : QColor("#66708f");
    if (option.activeSubControls.testFlag(control)) {
      color = option.state.testFlag(QStyle::State_Sunken) ? QColor("#a98cff")
                                                          : QColor("#ffffff");
    }
    drawChevron(
        painter,
        style()->subControlRect(QStyle::CC_SpinBox, &option, control, this), up,
        color);
  }
}

} // namespace wfgui
