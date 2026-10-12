package com.thomas.mobileagent;

import android.content.Context;

import java.io.InputStream;
import java.security.KeyStore;
import java.security.SecureRandom;
import java.security.cert.Certificate;
import java.security.cert.CertificateFactory;
import java.util.ArrayList;
import java.util.List;

import javax.net.ssl.SSLContext;
import javax.net.ssl.SSLSocketFactory;
import javax.net.ssl.TrustManagerFactory;
import javax.net.ssl.X509TrustManager;

final class ThomasTls {
    private ThomasTls() {}

    static final class Bundle {
        final SSLSocketFactory socketFactory;
        final X509TrustManager trustManager;
        Bundle(SSLSocketFactory socketFactory, X509TrustManager trustManager) {
            this.socketFactory = socketFactory;
            this.trustManager = trustManager;
        }
    }

    static Bundle create(Context context) throws Exception {
        TrustManagerFactory systemFactory = TrustManagerFactory.getInstance(
            TrustManagerFactory.getDefaultAlgorithm()
        );
        systemFactory.init((KeyStore) null);

        X509TrustManager system = null;
        for (javax.net.ssl.TrustManager item : systemFactory.getTrustManagers()) {
            if (item instanceof X509TrustManager) {
                system = (X509TrustManager) item;
                break;
            }
        }
        if (system == null) throw new IllegalStateException("Trust manager do sistema não disponível");

        X509TrustManager pinned = null;
        try {
            CertificateFactory cf = CertificateFactory.getInstance("X.509");
            int id = context.getResources().getIdentifier(
                "thomas_server_crt_pem", "raw", context.getPackageName()
            );
            if (id != 0) {
                InputStream in = context.getResources().openRawResource(id);
                Certificate cert;
                try { cert = cf.generateCertificate(in); } finally { in.close(); }

                KeyStore ks = KeyStore.getInstance(KeyStore.getDefaultType());
                ks.load(null, null);
                ks.setCertificateEntry("thomas-server", cert);

                TrustManagerFactory pinnedFactory = TrustManagerFactory.getInstance(
                    TrustManagerFactory.getDefaultAlgorithm()
                );
                pinnedFactory.init(ks);
                for (javax.net.ssl.TrustManager item : pinnedFactory.getTrustManagers()) {
                    if (item instanceof X509TrustManager) {
                        pinned = (X509TrustManager) item;
                        break;
                    }
                }
            }
        } catch (Throwable ignored) {
            // O trust store do sistema continua sendo suficiente para endpoints HTTPS públicos.
        }

        final X509TrustManager systemFinal = system;
        final X509TrustManager pinnedFinal = pinned;
        X509TrustManager combined = new X509TrustManager() {
            @Override public void checkClientTrusted(java.security.cert.X509Certificate[] chain, String authType)
                    throws java.security.cert.CertificateException {
                try {
                    systemFinal.checkClientTrusted(chain, authType);
                } catch (java.security.cert.CertificateException e) {
                    if (pinnedFinal == null) throw e;
                    pinnedFinal.checkClientTrusted(chain, authType);
                }
            }

            @Override public void checkServerTrusted(java.security.cert.X509Certificate[] chain, String authType)
                    throws java.security.cert.CertificateException {
                try {
                    systemFinal.checkServerTrusted(chain, authType);
                } catch (java.security.cert.CertificateException e) {
                    if (pinnedFinal == null) throw e;
                    pinnedFinal.checkServerTrusted(chain, authType);
                }
            }

            @Override public java.security.cert.X509Certificate[] getAcceptedIssuers() {
                List<java.security.cert.X509Certificate> all = new ArrayList<>();
                for (java.security.cert.X509Certificate c : systemFinal.getAcceptedIssuers()) all.add(c);
                if (pinnedFinal != null) {
                    for (java.security.cert.X509Certificate c : pinnedFinal.getAcceptedIssuers()) all.add(c);
                }
                return all.toArray(new java.security.cert.X509Certificate[0]);
            }
        };

        SSLContext ssl = SSLContext.getInstance("TLS");
        ssl.init(null, new javax.net.ssl.TrustManager[]{combined}, new SecureRandom());
        return new Bundle(ssl.getSocketFactory(), combined);
    }
}
