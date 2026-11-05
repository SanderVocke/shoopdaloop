use super::SocketLayout;

/// Helper for registering sockets aligned with rows inside an [`egui::Grid`].
///
/// Created by [`SocketLayout::grid`]. Each call to [`SocketGrid::row`] adds
/// content cells to the grid and registers input/output sockets at the row's
/// cross-axis center.
pub struct SocketGrid<'a> {
    layout: &'a mut SocketLayout,
}

impl SocketLayout {
    /// Render content inside an [`egui::Grid`], registering sockets aligned
    /// with individual rows via [`SocketGrid::row`].
    ///
    /// The caller provides a pre-configured [`egui::Grid`] for full control
    /// over spacing, striping, etc. Widgets placed within each
    /// [`SocketGrid::row`] closure become grid cells, and `egui::Grid` aligns
    /// columns across rows.
    pub fn grid<R>(
        &mut self,
        grid: egui::Grid,
        ui: &mut egui::Ui,
        content: impl FnOnce(&mut SocketGrid<'_>, &mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        grid.show(ui, |ui| {
            let mut sg = SocketGrid { layout: self };
            content(&mut sg, ui)
        })
    }
}

impl SocketGrid<'_> {
    /// Add a row of content to the grid, optionally registering input and
    /// output sockets at the row's cross-axis center.
    ///
    /// Widgets added by `content` become cells in the current grid row.
    /// After the content, `ui.end_row()` is called automatically.
    pub fn row<R>(
        &mut self,
        ui: &mut egui::Ui,
        input: Option<usize>,
        output: Option<usize>,
        content: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R {
        let row_top = ui.cursor().top();
        let r = content(ui);
        ui.end_row();
        let next_row_top = ui.cursor().top();
        let spacing = ui.spacing().item_spacing.y;
        let cross = row_top + (next_row_top - row_top - spacing) / 2.0;
        if let Some(ix) = input {
            self.layout.input_at(ix, cross);
        }
        if let Some(ix) = output {
            self.layout.output_at(ix, cross);
        }
        r
    }
}
