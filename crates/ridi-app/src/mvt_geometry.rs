use geozero::error::Result;
use geozero::GeomProcessor;

#[derive(Debug, Default, Clone)]
pub struct MvtGeometryLines {
    lines: Vec<Vec<(f64, f64)>>,
    current_line: Vec<(f64, f64)>,
}

impl MvtGeometryLines {
    pub fn lines(self) -> Vec<Vec<(f64, f64)>> {
        self.lines
    }

    fn finish_current_line(&mut self) {
        if !self.current_line.is_empty() {
            self.lines.push(std::mem::take(&mut self.current_line));
        }
    }
}

impl GeomProcessor for MvtGeometryLines {
    fn point_begin(&mut self, _idx: usize) -> Result<()> {
        self.current_line.clear();
        Ok(())
    }

    fn point_end(&mut self, _idx: usize) -> Result<()> {
        self.finish_current_line();
        Ok(())
    }

    fn multipoint_begin(&mut self, _size: usize, _idx: usize) -> Result<()> {
        self.current_line.clear();
        Ok(())
    }

    fn multipoint_end(&mut self, _idx: usize) -> Result<()> {
        self.finish_current_line();
        Ok(())
    }

    fn linestring_begin(&mut self, _tagged: bool, _size: usize, _idx: usize) -> Result<()> {
        self.current_line.clear();
        Ok(())
    }

    fn linestring_end(&mut self, _tagged: bool, _idx: usize) -> Result<()> {
        self.finish_current_line();
        Ok(())
    }

    fn xy(&mut self, x: f64, y: f64, _idx: usize) -> Result<()> {
        self.current_line.push((x, y));
        Ok(())
    }
}
