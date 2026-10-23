// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT
//
// 本文件是 JC Shell 的 fork 扩展（上游无此文件）。

package app.tauri.biometric

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.hardware.fingerprint.FingerprintManager
import android.os.Bundle
import android.os.CancellationSignal
import android.os.Handler
import android.os.Looper
import android.view.View
import android.widget.Button
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.biometric.BiometricPrompt

/**
 * 「只认指纹」专用提示页（JC Shell fork 扩展）。
 *
 * 为什么需要它：BiometricPrompt 只能限定认证**强度**（STRONG / WEAK / DEVICE_CREDENTIAL），
 * 没有模态开关 —— 华为等机型的 3D 人脸属于强级（class-3），即使传 BIOMETRIC_STRONG 也满足要求，
 * 系统仍会按自己的偏好弹出人脸；部分 ROM 甚至直接忽略应用传入的强度（实测：华为 + 鸿蒙兼容层
 * 上「仅开启指纹」仍会刷脸）。只有已废弃但仍在的 FingerprintManager 直连指纹服务，
 * 才能做到真正的「只认指纹」；代价是它不提供任何 UI，所以这里自绘一个极简提示页。
 *
 * 不可用时（容器 / 机型没有指纹服务、未录入指纹）由 BiometricPlugin 回落到 BiometricPrompt，
 * 行为与上游一致，不会更差。
 */
@Suppress("DEPRECATION")
class FingerprintActivity : AppCompatActivity() {
  private var signal: CancellationSignal? = null

  /** 是否已经结束（成功 / 失败 / 取消只允许上报一次） */
  private var done = false

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    setContentView(R.layout.fingerprint_activity)

    val title = intent.getStringExtra(BiometricPlugin.TITLE)
    findViewById<TextView>(R.id.fp_title).text =
      if (title.isNullOrEmpty()) "请使用指纹验证" else title

    val subtitle = intent.getStringExtra(BiometricPlugin.SUBTITLE)
      ?: intent.getStringExtra(BiometricPlugin.REASON)
    findViewById<TextView>(R.id.fp_subtitle).apply {
      text = subtitle ?: ""
      visibility = if (subtitle.isNullOrEmpty()) View.GONE else View.VISIBLE
    }

    val cancelTitle = intent.getStringExtra(BiometricPlugin.CANCEL_TITLE)
    findViewById<Button>(R.id.fp_cancel).apply {
      text = if (cancelTitle.isNullOrEmpty()) "取消" else cancelTitle
      setOnClickListener {
        finishWith(
          BiometryResultType.ERROR,
          BiometricPrompt.ERROR_USER_CANCELED,
          "Cancel button was pressed"
        )
      }
    }

    startListening()
  }

  private fun startListening() {
    val manager = getSystemService(Context.FINGERPRINT_SERVICE) as? FingerprintManager
    if (manager == null || !manager.isHardwareDetected || !manager.hasEnrolledFingerprints()) {
      // 指纹服务不可用：把错误如实上报，让上层有机会回落到 BiometricPrompt
      finishWith(
        BiometryResultType.ERROR,
        BiometricPrompt.ERROR_HW_UNAVAILABLE,
        "Fingerprint service unavailable"
      )
      return
    }

    val cancellation = CancellationSignal()
    signal = cancellation
    manager.authenticate(
      null,
      cancellation,
      0,
      object : FingerprintManager.AuthenticationCallback() {
        override fun onAuthenticationSucceeded(result: FingerprintManager.AuthenticationResult?) {
          finishWith(BiometryResultType.SUCCESS, 0, "")
        }

        override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
          finishWith(BiometryResultType.ERROR, mapErrorCode(errorCode), errString.toString())
        }

        override fun onAuthenticationFailed() {
          // 单次不匹配（手指没放正 / 不认识的指纹）：系统会继续等待重试，这里不结束，
          // 与 BiometricPrompt 的 onAuthenticationFailed 语义保持一致。
        }
      },
      Handler(Looper.getMainLooper())
    )
  }

  /** FingerprintManager 的错误码与 BiometricPrompt 基本同号，这里显式映射一遍避免歧义 */
  private fun mapErrorCode(errorCode: Int): Int = when (errorCode) {
    FingerprintManager.FINGERPRINT_ERROR_HW_UNAVAILABLE -> BiometricPrompt.ERROR_HW_UNAVAILABLE
    FingerprintManager.FINGERPRINT_ERROR_HW_NOT_PRESENT -> BiometricPrompt.ERROR_HW_NOT_PRESENT
    FingerprintManager.FINGERPRINT_ERROR_LOCKOUT -> BiometricPrompt.ERROR_LOCKOUT
    FingerprintManager.FINGERPRINT_ERROR_LOCKOUT_PERMANENT -> BiometricPrompt.ERROR_LOCKOUT_PERMANENT
    FingerprintManager.FINGERPRINT_ERROR_TIMEOUT -> BiometricPrompt.ERROR_TIMEOUT
    FingerprintManager.FINGERPRINT_ERROR_CANCELED -> BiometricPrompt.ERROR_CANCELED
    FingerprintManager.FINGERPRINT_ERROR_USER_CANCELED -> BiometricPrompt.ERROR_USER_CANCELED
    // 注：FingerprintManager 没有 NO_BIOMETRICS 常量（该错误码只在 BiometricPrompt 侧），
    // 未录入指纹的情况已由 startListening 的前置检查兜住，这里走默认分支即可。
    else -> BiometricPrompt.ERROR_UNABLE_TO_PROCESS
  }

  /** 结束并回传结果，协议与 BiometricActivity 完全一致（由 BiometricPlugin 统一解析） */
  private fun finishWith(type: BiometryResultType, errorCode: Int, errorMessage: String) {
    if (done) return
    done = true
    signal?.cancel()
    signal = null
    val result = Intent()
    val prefix = BiometricPlugin.RESULT_EXTRA_PREFIX
    result
      .putExtra(prefix + BiometricPlugin.RESULT_TYPE, type.toString())
      .putExtra(prefix + BiometricPlugin.RESULT_ERROR_CODE, errorCode)
      .putExtra(prefix + BiometricPlugin.RESULT_ERROR_MESSAGE, errorMessage)
    setResult(Activity.RESULT_OK, result)
    finish()
  }

  override fun onDestroy() {
    // 用户按返回键 / 进程回收：取消监听，避免回调打到已销毁的 Activity
    signal?.cancel()
    signal = null
    super.onDestroy()
  }

  override fun onBackPressed() {
    finishWith(
      BiometryResultType.ERROR,
      BiometricPrompt.ERROR_USER_CANCELED,
      "Cancel button was pressed"
    )
  }
}
