// 修复 iOS 安全区：让 WKWebView 真正铺满整屏，把上下安全区交给页面用 env() 处理。
//
// 【为什么需要这个文件】
// tao 0.35（Tauri 的窗口库）在 iOS 上把窗口 inner_size 报成「安全区尺寸」：
//   tao-0.35.3/src/platform_impl/ios/window.rs:211
//     pub fn inner_size(&self) -> PhysicalSize<u32> {
//         let safe_area = self.safe_area_screen_space();   // 安全区尺寸
//         PhysicalSize::new(safe_area.size.width, safe_area.size.height)
//     }
// wry 用它设置 WKWebView 的 frame，于是 WebView 只有「屏幕高 − 顶部安全区 − 底部安全区」，
// 底部露出原生窗口背景（深色）→ 表现为「底部导航栏下方一条黑边」，页面无法绘制。
// 另外 WKWebView 默认会自动为安全区加 contentInset，页面布局视口仍被缩进，所以还要
// 关掉 contentInsetAdjustmentBehavior（安全区改由页面的 env() 自行处理）。
//
// 【做法】
//   1) swizzle `-[WKWebView didMoveToWindow]`：把 WebView 及其**整条祖先链**逐层铺满；
//   2) contentInsetAdjustmentBehavior = Never（关掉系统自动缩进）；
//   3) 启动后延迟多次再纠正（防止 wry/tao 之后又设置一次 frame）。
//
// ⚠️ scrollView 的绿色仅为诊断用：修好后被页面完全盖住不可见；若仍看到绿色，
//    说明 WebView 虽已铺满但页面未铺满（用于定位问题），定位后可删。

#import <UIKit/UIKit.h>
#import <WebKit/WebKit.h>
#import <objc/runtime.h>

static IMP jc_orig_didMoveToWindow = NULL;

/// 把 WebView 及其祖先链逐层铺满父视图（自顶向下，保证每层拿到正确的 superview.bounds）
static void jc_fill_ancestors(WKWebView *web) {
  UIWindow *win = web.window;
  if (win == nil) {
    return;
  }
  NSMutableArray<UIView *> *chain = [NSMutableArray array];
  for (UIView *v = web; v != nil && v != win; v = v.superview) {
    [chain addObject:v];
  }
  for (UIView *v in [chain reverseObjectEnumerator]) {
    if (v.superview != nil) {
      v.frame = v.superview.bounds;
    }
  }
  web.autoresizingMask =
      UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;

  // 关掉系统对安全区的自动缩进：上下安全区交给页面（viewport-fit=cover + env()）
  web.scrollView.contentInsetAdjustmentBehavior =
      UIScrollViewContentInsetAdjustmentNever;
  web.scrollView.contentInset = UIEdgeInsetsZero;
  web.scrollView.scrollIndicatorInsets = UIEdgeInsetsZero;
}

/// 遍历所有 window，找出全部 WKWebView 并修正
static void jc_fix_all_webviews(void) {
  for (UIScene *scene in [UIApplication sharedApplication].connectedScenes) {
    if (![scene isKindOfClass:[UIWindowScene class]]) {
      continue;
    }
    for (UIWindow *win in ((UIWindowScene *)scene).windows) {
      NSMutableArray<UIView *> *stack = [NSMutableArray arrayWithObject:(UIView *)win];
      while (stack.count > 0) {
        UIView *v = stack.lastObject;
        [stack removeLastObject];
        if ([v isKindOfClass:[WKWebView class]]) {
          jc_fill_ancestors((WKWebView *)v);
        }
        for (UIView *sub in v.subviews) {
          [stack addObject:sub];
        }
      }
    }
  }
}

static void jc_didMoveToWindow(WKWebView *self, SEL _cmd) {
  if (jc_orig_didMoveToWindow) {
    ((void (*)(id, SEL))jc_orig_didMoveToWindow)(self, _cmd);
  }
  jc_fill_ancestors(self);
}

__attribute__((constructor)) static void jc_install_safe_area_fix(void) {
  Class cls = [WKWebView class];
  if (cls) {
    SEL sel = @selector(didMoveToWindow);
    Method m = class_getInstanceMethod(cls, sel);
    if (m) {
      const char *types = method_getTypeEncoding(m);
      IMP orig = method_getImplementation(m);
      if (class_addMethod(cls, sel, (IMP)jc_didMoveToWindow, types)) {
        jc_orig_didMoveToWindow = orig;  // 原实现来自父类，未被改动
      } else {
        jc_orig_didMoveToWindow = orig;
        method_setImplementation(m, (IMP)jc_didMoveToWindow);
      }
    }
  }
  dispatch_async(dispatch_get_main_queue(), ^{
    jc_fix_all_webviews();
  });
  for (NSNumber *delay in @[ @0.2, @1.0, @3.0 ]) {
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW,
                                (int64_t)(delay.doubleValue * NSEC_PER_SEC)),
                   dispatch_get_main_queue(), ^{
                     jc_fix_all_webviews();
                   });
  }
}
