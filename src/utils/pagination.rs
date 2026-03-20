use super::PAGE_SIZE;

/// Calculate page number and offset from optional page parameter.
pub fn calc_page_offset(page: Option<i64>) -> (i64, i64) {
    let page = page.unwrap_or(1).clamp(1, 100_000);
    let offset = (page - 1) * PAGE_SIZE;
    (page, offset)
}

/// Calculate total pages from total record count.
pub fn calc_total_pages(total: i64) -> i64 {
    (total + PAGE_SIZE - 1) / PAGE_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calc_page_offset_defaults() {
        let (page, offset) = calc_page_offset(None);
        assert_eq!(page, 1);
        assert_eq!(offset, 0);
    }

    #[test]
    fn test_calc_page_offset_page_2() {
        let (page, offset) = calc_page_offset(Some(2));
        assert_eq!(page, 2);
        assert_eq!(offset, 20);
    }

    #[test]
    fn test_calc_page_offset_clamps() {
        let (page, _) = calc_page_offset(Some(0));
        assert_eq!(page, 1);
        let (page, _) = calc_page_offset(Some(-5));
        assert_eq!(page, 1);
    }

    #[test]
    fn test_calc_total_pages() {
        assert_eq!(calc_total_pages(0), 0);
        assert_eq!(calc_total_pages(1), 1);
        assert_eq!(calc_total_pages(20), 1);
        assert_eq!(calc_total_pages(21), 2);
        assert_eq!(calc_total_pages(40), 2);
        assert_eq!(calc_total_pages(41), 3);
    }
}
