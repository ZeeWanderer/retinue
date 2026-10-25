#include "build_plan_widget.h"

#include <QFrame>
#include <QGridLayout>
#include <QHash>
#include <QJsonArray>
#include <QLabel>
#include <QVBoxLayout>
#include <utility>

#include "mod_card_widget.h"
#include "widget_capture.h"

namespace {
QString polarityName(const QString &value) {
  if (value.isEmpty() || value == "none") {
    return "Unpolarized";
  }
  return value.first(1).toUpper() + value.sliced(1);
}

QWidget *polarityLayout(const QJsonObject &baseline, const QJsonObject &result) {
  auto *widget = new QWidget;
  auto *layout = new QVBoxLayout(widget);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(6);
  auto *heading = new QLabel(result.isEmpty() ? "Current polarities" : "Planned polarities");
  heading->setObjectName("sectionTitle");
  layout->addWidget(heading);
  QHash<QString, QJsonObject> polarities;
  const QJsonArray entries = result.isEmpty()
                                ? baseline.value("effective_polarities").toArray()
                                : result.value("final_polarities").toArray();
  for (const auto &entry : entries) {
    const auto object = entry.toObject();
    polarities.insert(object.value("slot_id").toString(), object);
  }
  for (const auto &regionValue : baseline.value("topology").toObject().value("regions").toArray()) {
    const auto region = regionValue.toObject();
    auto *grid = new QGridLayout;
    grid->setSpacing(4);
    int index = 0;
    const int columns = qMax(1, region.value("columns").toInt(4));
    for (const auto &value : region.value("slots").toArray()) {
      const auto slot = value.toObject();
      if (!slot.value("planner").toBool()) {
        continue;
      }
      const QString id = slot.value("id").toString();
      const auto state = polarities.value(id);
      const QString polarity = state.value("polarity").toString("none");
      auto *cell = new QFrame;
      cell->setObjectName("buildPolarityCell");
      cell->setProperty("changed", state.value("changed").toBool());
      cell->setProperty("slotId", id);
      cell->setProperty("polarity", polarity);
      cell->setFixedSize(78, 48);
      const QString label = slot.value("label").toString(id);
      cell->setToolTip(label + ": " + polarityName(polarity));
      auto *cellLayout = new QVBoxLayout(cell);
      cellLayout->setContentsMargins(4, 4, 4, 3);
      cellLayout->setSpacing(1);
      auto *icon = new QLabel;
      icon->setAlignment(Qt::AlignCenter);
      if (polarity == "none") {
        icon->setText("-");
      } else {
        icon->setPixmap(wfgui::modPolarityPixmap(polarity, QColor("#ffffff"))
                            .scaled(20, 20, Qt::KeepAspectRatio, Qt::SmoothTransformation));
      }
      auto *name = new QLabel(label);
      name->setObjectName("buildPolarityLabel");
      name->setAlignment(Qt::AlignCenter);
      cellLayout->addWidget(icon);
      cellLayout->addWidget(name);
      grid->addWidget(cell, index / columns, index % columns);
      ++index;
    }
    if (index > 0) {
      layout->addLayout(grid);
    } else {
      delete grid;
    }
  }
  layout->addStretch();
  return widget;
}
} // namespace

BuildPlanWidget::BuildPlanWidget(QWidget *parent)
    : QWidget(parent), layout_(new QVBoxLayout(this)) {
  setObjectName("buildPlan");
  wfgui::setCaptureTarget(this, "build-planner.plan");
  layout_->setContentsMargins(0, 0, 0, 0);
}

void BuildPlanWidget::setPlan(const QJsonObject &baseline, const QJsonObject &result) {
  if (body_ && baseline == baseline_ && result == result_) {
    return;
  }
  baseline_ = baseline;
  result_ = result;
  delete body_;
  body_ = new QWidget;
  layout_->addWidget(body_);
  auto *layout = new QVBoxLayout(body_);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(10);
  if (baseline.isEmpty()) {
    return;
  }
  const bool ready = result.value("status").toString() == "ready";
  const auto operations = result.value("operations").toArray();
  const bool unchanged = ready && operations.isEmpty();
  if (!unchanged) {
    auto *polarityRow = new QHBoxLayout;
    polarityRow->setSpacing(24);
    polarityRow->addStretch();
    polarityRow->addWidget(polarityLayout(baseline, {}));
    if (ready) {
      polarityRow->addWidget(polarityLayout(baseline, result));
    }
    polarityRow->addStretch();
    layout->addLayout(polarityRow);
  }
  if (!ready) {
    return;
  }

  const auto requirements = result.value("forma_requirements").toObject();
  QStringList materials;
  for (const auto &[key, label] : {std::pair{"standard", "Forma"},
                                  std::pair{"omni", "Omni Forma"},
                                  std::pair{"umbral", "Umbral Forma"}}) {
    if (const int count = requirements.value(key).toInt(); count > 0) {
      materials.append(QString("%1 %2").arg(count).arg(label));
    }
  }
  auto *summary = new QLabel(unchanged ? "No polarity changes needed"
                            : materials.isEmpty() ? "No Forma needed" : materials.join(" + "));
  summary->setObjectName("buildPlanSummary");
  layout->addWidget(summary);
  if (operations.isEmpty()) {
    auto *message = new QLabel("Current polarities support every build in this group.");
    message->setObjectName("secondaryText");
    message->setWordWrap(true);
    layout->addWidget(message);
  }
  int index = 1;
  for (const auto &value : operations) {
    const auto operation = value.toObject();
    QString text;
    if (operation.value("action").toString() == "swap") {
      text = QString("Swap %1 (%2) with %3 (%4)")
                 .arg(operation.value("label").toString(), polarityName(operation.value("before").toString()),
                      operation.value("other_label").toString(), polarityName(operation.value("polarity").toString()));
    } else {
      const QString kind = operation.value("forma").toString();
      const QString material = kind == "omni" ? "Omni Forma" : kind == "umbral" ? "Umbral Forma" : "Forma";
      text = QString("Apply %1 to %2: %3 to %4")
                 .arg(material, operation.value("label").toString(),
                      polarityName(operation.value("before").toString()),
                      polarityName(operation.value("polarity").toString()));
    }
    auto *label = new QLabel(QString("%1. %2").arg(index++).arg(text));
    label->setObjectName("buildPlanOperation");
    label->setWordWrap(true);
    layout->addWidget(label);
  }
}
