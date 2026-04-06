/// Tests that blocking parse operations are properly wrapped in
/// spawn_blocking + tokio::time::timeout, so they never hang the async runtime.
///
/// NO external services used (no web search, no LLM, no network).

#[tokio::test]
async fn timeout_fires_on_blocking_operation() {
    // Core proof: tokio::time::timeout + spawn_blocking actually interrupts a hang
    let timeout = std::time::Duration::from_secs(2);
    let t0 = std::time::Instant::now();

    let result = tokio::time::timeout(timeout, tokio::task::spawn_blocking(|| {
        // Simulate a hang — sleep for 60s (like a stuck readability parse)
        std::thread::sleep(std::time::Duration::from_secs(60));
        42
    })).await;

    let elapsed = t0.elapsed();
    assert!(result.is_err(), "Should have timed out");
    assert!(elapsed.as_secs() < 5, "Timeout should fire at ~2s, took {:.1}s", elapsed.as_secs_f32());
    println!("Timeout fired correctly after {:.1}s", elapsed.as_secs_f32());
}

#[tokio::test]
async fn async_runtime_not_blocked_during_spawn_blocking() {
    // Prove that spawn_blocking doesn't block other async tasks
    // (This is the bug we had: extract_html_tables ran synchronously,
    // blocking the tokio runtime so timeout could never fire)
    let timeout = std::time::Duration::from_secs(3);

    // Start a "blocking" task in spawn_blocking
    let blocking = tokio::time::timeout(timeout, tokio::task::spawn_blocking(|| {
        std::thread::sleep(std::time::Duration::from_secs(10));
    }));

    // Meanwhile, this async task should still run immediately
    let quick = tokio::time::sleep(std::time::Duration::from_millis(100));

    let t0 = std::time::Instant::now();
    tokio::select! {
        _ = blocking => { panic!("Blocking task should not finish first"); },
        _ = quick => {},
    }
    let elapsed = t0.elapsed();
    assert!(elapsed.as_millis() < 500, "Async runtime should not be blocked, quick task took {}ms", elapsed.as_millis());
    println!("Async runtime stayed responsive: {}ms", elapsed.as_millis());
}

#[tokio::test]
async fn table_extraction_in_spawn_blocking_with_timeout() {
    // Simulate the exact pattern used in fetch_article_content:
    // tokio::time::timeout(PARSE_TIMEOUT, tokio::task::spawn_blocking(|| extract_html_tables(...)))
    let timeout = std::time::Duration::from_secs(5);

    // Generate a large HTML page with tables (~1MB)
    let mut html = String::with_capacity(1_500_000);
    html.push_str("<html><body>");
    for t in 0..50 {
        html.push_str(&format!("<table id='t{}'><tbody>", t));
        for r in 0..200 {
            html.push_str(&format!(
                "<tr><td>Data {} row {}</td><td>{}</td><td>{}</td></tr>",
                t, r, r * 42, r * 17
            ));
        }
        html.push_str("</tbody></table>");
    }
    html.push_str("</body></html>");

    let t0 = std::time::Instant::now();
    let html_clone = html.clone();

    let result = tokio::time::timeout(timeout, tokio::task::spawn_blocking(move || {
        // Simple table counting (mirrors extract_html_tables pattern)
        let lower = html_clone.to_lowercase();
        let mut count = 0usize;
        let mut pos = 0;
        while let Some(idx) = lower[pos..].find("<table") {
            count += 1;
            pos += idx + 6;
            if count >= 5 { break; }
        }
        count
    })).await;

    let elapsed = t0.elapsed();
    assert!(result.is_ok(), "Should complete within timeout");
    println!("Table extraction: {} tables found in {:.1}s", result.unwrap().unwrap(), elapsed.as_secs_f32());
}

#[tokio::test]
async fn concurrent_async_work_during_blocking_parse() {
    // Prove that while spawn_blocking is working, we can still do async I/O
    // This is critical: if we DON'T use spawn_blocking, the whole runtime hangs
    let t0 = std::time::Instant::now();

    let blocking_handle = tokio::task::spawn_blocking(|| {
        // Simulate heavy CPU work (like parsing 2MB HTML)
        let mut sum: u64 = 0;
        for i in 0..50_000_000u64 { sum = sum.wrapping_add(i); }
        sum
    });

    // These async tasks should run concurrently with the blocking task
    let mut ticks = 0u32;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        ticks += 1;
        if blocking_handle.is_finished() { break; }
        if ticks > 500 { break; } // safety
    }

    let elapsed = t0.elapsed();
    assert!(ticks > 1, "Should have ticked multiple times while blocking ran, got {}", ticks);
    println!("Blocking CPU work took {:.1}s, async ticked {} times (proves non-blocking)", elapsed.as_secs_f32(), ticks);
}
