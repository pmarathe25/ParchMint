#pragma once

#include <QString>

// Deliberately narrow adapter: Rust supplies already-normalized compile text
// and physical page settings. No project/domain state crosses into C++.
bool parchmint_render_pdf_qt(const QString& destination,
                             const QString& html,
                             int width_micrometres,
                             int height_micrometres,
                             int margin_left_micrometres,
                             int margin_top_micrometres,
                             int margin_right_micrometres,
                             int margin_bottom_micrometres);
