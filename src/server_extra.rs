#[derive(serde::Deserialize, Default)]
struct RunStatusQuery {
    #[serde(default)] session_id: String,
    #[serde(default)] run_id: String,
}
async fn dsh_status(State(state):State<Arc<AppState>>, Query(q):Query<RunStatusQuery>)->Response {
    if q.run_id.len()>160 || q.session_id.len()>160 {
        return (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"session_id/run_id exceeds its limit"}))).into_response();
    }
    let active=if !q.run_id.is_empty(){state.runs.lock().await.contains_key(&q.run_id)}else{
        let ids={let requests=state.run_requests.lock().await;requests.iter().filter(|(_,r)|r.session_id==q.session_id).map(|(id,_)|id.clone()).collect::<Vec<_>>()};
        let runs=state.runs.lock().await;
        ids.iter().any(|id|runs.contains_key(id))
    };
    let history=if q.session_id.is_empty(){Vec::new()}else{state.memory.history(&q.session_id,2).unwrap_or_default()};
    let answer=history.iter().rev().find(|item|item["role"]=="assistant").and_then(|item|item["content"].as_str()).unwrap_or("").to_owned();
    Json(json!({"ok":true,"active":active,"session_id":q.session_id,"run_id":q.run_id,"answer":if active{String::new()}else{answer},"error":Value::Null})).into_response()
}

fn local_proxy_error()->Response {
    (StatusCode::BAD_GATEWAY,Json(json!({"ok":false,"error":"upstream_unavailable","message":"O serviço local não respondeu dentro do prazo."}))).into_response()
}
async fn proxy_local(_state:&Arc<AppState>,url:String,method:&str,body:Option<Bytes>,cap:usize,timeout_seconds:u64)->Response {
    let client=match reqwest::Client::builder().connect_timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(timeout_seconds)).build(){
        Ok(client)=>client,Err(_)=>return local_proxy_error(),
    };
    let request=match method {
        "POST"=>client.post(&url).header(header::CONTENT_TYPE,"application/json").body(body.unwrap_or_default()),
        _=>client.get(&url),
    };
    let upstream=match tokio::time::timeout(Duration::from_secs(timeout_seconds+1),request.send()).await {
        Ok(Ok(response))=>response,_=>return local_proxy_error(),
    };
    let status=StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type=upstream.headers().get(header::CONTENT_TYPE).and_then(|v|v.to_str().ok()).unwrap_or("application/octet-stream").to_owned();
    if upstream.content_length().is_some_and(|n|n>cap as u64){return (StatusCode::PAYLOAD_TOO_LARGE,Json(json!({"ok":false,"error":"upstream_response_too_large"}))).into_response();}
    let mut upstream=upstream;
    let mut bytes=Vec::new();
    loop {
        let next=match tokio::time::timeout(Duration::from_secs(timeout_seconds),upstream.chunk()).await {
            Ok(Ok(chunk))=>chunk,_=>return local_proxy_error(),
        };
        let Some(chunk)=next else{break};
        if bytes.len().saturating_add(chunk.len())>cap{return (StatusCode::PAYLOAD_TOO_LARGE,Json(json!({"ok":false,"error":"upstream_response_too_large"}))).into_response();}
        bytes.extend_from_slice(&chunk);
    }
    Response::builder().status(status).header(header::CONTENT_TYPE,content_type).header(header::CACHE_CONTROL,"no-store")
        .body(Body::from(bytes)).unwrap_or_else(|_|StatusCode::BAD_GATEWAY.into_response())
}
async fn browser_frame(State(state):State<Arc<AppState>>)->Response {
    proxy_local(&state,"http://127.0.0.1:8095/frame".into(),"GET",None,5*1024*1024,5).await
}
async fn mobile_frame(State(state):State<Arc<AppState>>,Query(q):Query<std::collections::HashMap<String,String>>)->Response {
    let id=q.get("deviceId").map(String::as_str).unwrap_or("").trim();
    if id.is_empty()||id.len()>180{return (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"deviceId é obrigatório e deve ter até 180 bytes."}))).into_response();}
    let mut url=reqwest::Url::parse("http://127.0.0.1:3198/frame").unwrap();
    url.query_pairs_mut().append_pair("deviceId",id);
    proxy_local(&state,url.to_string(),"GET",None,8*1024*1024,6).await
}
async fn mobile_apk(State(state):State<Arc<AppState>>)->Response {
    let path=state.config.data_dir.join("mobile/ThomasMobileAgent.apk");
    let meta=match tokio::fs::metadata(&path).await{Ok(m) if m.is_file()=>m,_=>return (StatusCode::NOT_FOUND,Json(json!({"ok":false,"error":"O APK do Thomas Mobile não foi preparado para distribuição pelo Harness."}))).into_response()};
    if meta.len()>100*1024*1024{return (StatusCode::PAYLOAD_TOO_LARGE,Json(json!({"ok":false,"error":"APK exceeds 100 MiB"}))).into_response();}
    let bytes=match tokio::fs::read(&path).await{Ok(b)=>b,_=>return (StatusCode::NOT_FOUND,Json(json!({"ok":false,"error":"O APK do Thomas Mobile não está acessível ao serviço."}))).into_response()};
    Response::builder().status(StatusCode::OK).header(header::CONTENT_TYPE,"application/vnd.android.package-archive")
        .header(header::CONTENT_DISPOSITION,"attachment; filename=\"ThomasMobileAgent.apk\"").header(header::CACHE_CONTROL,"no-store")
        .body(Body::from(bytes)).unwrap_or_else(|_|StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
async fn voice_config(State(state):State<Arc<AppState>>)->Response{proxy_local(&state,"http://127.0.0.1:1342/api/voice/config".into(),"GET",None,1024*1024,7).await}
async fn voice_config_post(State(state):State<Arc<AppState>>,body:Bytes)->Response{if body.len()>2*1024*1024{return (StatusCode::PAYLOAD_TOO_LARGE,Json(json!({"ok":false,"error":"voice config is limited to 2 MiB"}))).into_response();}proxy_local(&state,"http://127.0.0.1:1342/api/voice/config".into(),"POST",Some(body),2*1024*1024,10).await}
async fn voice_voices(State(state):State<Arc<AppState>>)->Response{proxy_local(&state,"http://127.0.0.1:1342/api/voice/voices".into(),"GET",None,1024*1024,7).await}
async fn voice_speak(State(state):State<Arc<AppState>>,body:Bytes)->Response {
    if body.len()>2*1024*1024{return (StatusCode::PAYLOAD_TOO_LARGE,Json(json!({"ok":false,"error":"voice request is limited to 2 MiB"}))).into_response();}
    proxy_local(&state,"http://127.0.0.1:1342/api/voice/speak".into(),"POST",Some(body),16*1024*1024,55).await
}
async fn voice_root(State(state):State<Arc<AppState>>)->Response{proxy_local(&state,"http://127.0.0.1:1342/".into(),"GET",None,1024*1024,7).await}
async fn voice_root_post(State(state):State<Arc<AppState>>,body:Bytes)->Response {
    if body.len()>2*1024*1024{return (StatusCode::PAYLOAD_TOO_LARGE,Json(json!({"ok":false,"error":"voice request is limited to 2 MiB"}))).into_response();}
    proxy_local(&state,"http://127.0.0.1:1342/".into(),"POST",Some(body),16*1024*1024,55).await
}
async fn voice_proxy(State(state):State<Arc<AppState>>,Path(path):Path<String>,uri:axum::extract::OriginalUri)->Response {
    if path.len()>1000||path.split('/').any(|p|p==".."||p.contains('\\')||p.contains('#')){return (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"invalid voice path"}))).into_response();}
    let query=uri.0.query().map(|q|format!("?{q}")).unwrap_or_default();
    proxy_local(&state,format!("http://127.0.0.1:1342/{path}{query}"),"GET",None,16*1024*1024,10).await
}
async fn voice_proxy_post(State(state):State<Arc<AppState>>,Path(path):Path<String>,uri:axum::extract::OriginalUri,body:Bytes)->Response {
    if path.len()>1000||path.split('/').any(|p|p==".."||p.contains('\\')||p.contains('#')){return (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"invalid voice path"}))).into_response();}
    if body.len()>2*1024*1024{return (StatusCode::PAYLOAD_TOO_LARGE,Json(json!({"ok":false,"error":"voice request is limited to 2 MiB"}))).into_response();}
    let query=uri.0.query().map(|q|format!("?{q}")).unwrap_or_default();
    proxy_local(&state,format!("http://127.0.0.1:1342/{path}{query}"),"POST",Some(body),16*1024*1024,55).await
}
async fn terminal_raw_root()->Response {
    (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"file path required for terminal preview"}))).into_response()
}

async fn terminal_raw(State(state):State<Arc<AppState>>,Path(raw_path):Path<String>)->Response {
    if raw_path.len()>2000||raw_path.contains('\\'){return (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"invalid preview path"}))).into_response();}
    let relative=std::path::PathBuf::from(&raw_path);
    if relative.is_absolute()||relative.components().any(|c|matches!(c,std::path::Component::ParentDir|std::path::Component::RootDir|std::path::Component::Prefix(_))){return StatusCode::FORBIDDEN.into_response();}
    let root=match std::fs::canonicalize(&state.config.workspace){Ok(p)=>p,Err(_)=>return StatusCode::INTERNAL_SERVER_ERROR.into_response()};
    let path=match std::fs::canonicalize(root.join(relative)){Ok(p) if p.starts_with(&root)=>p,_=>return StatusCode::NOT_FOUND.into_response()};
    if !path.is_file(){return StatusCode::NOT_FOUND.into_response();}
    let components=path.strip_prefix(&root).unwrap_or(&path).components().filter_map(|c|c.as_os_str().to_str()).collect::<Vec<_>>();
    let filename=path.file_name().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase();
    if components.iter().any(|p|p.starts_with('.'))||[".env","providers.json","credentials.json","id_rsa","id_ed25519"].contains(&filename.as_str())||filename.ends_with(".db")||filename.ends_with(".db-wal")||filename.ends_with(".db-shm"){return StatusCode::FORBIDDEN.into_response();}
    const PREVIEW_EXTS:&[&str]=&["html","htm","css","scss","less","js","mjs","cjs","ts","tsx","jsx","vue","json","jsonc","yaml","yml","toml","md","mdx","txt","xml","svg","png","jpg","jpeg","gif","webp","avif","ico"];
    let ext=path.extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase();
    if !PREVIEW_EXTS.contains(&ext.as_str()){return StatusCode::FORBIDDEN.into_response();}
    let meta=match tokio::fs::metadata(&path).await{Ok(m)=>m,_=>return StatusCode::NOT_FOUND.into_response()};
    if meta.len()>8_000_000{return (StatusCode::PAYLOAD_TOO_LARGE,Json(json!({"ok":false,"error":"Ativo acima de 8 MB."}))).into_response();}
    let data=match tokio::fs::read(&path).await{Ok(b)=>b,_=>return StatusCode::FORBIDDEN.into_response()};
    let mime=match ext.as_str(){"html"|"htm"=>"text/html; charset=utf-8","css"|"scss"|"less"=>"text/css; charset=utf-8","js"|"mjs"|"cjs"=>"text/javascript; charset=utf-8","ts"|"tsx"|"jsx"|"vue"|"toml"|"md"|"mdx"|"txt"=>"text/plain; charset=utf-8","json"|"jsonc"=>"application/json; charset=utf-8","yaml"|"yml"=>"text/yaml; charset=utf-8","xml"=>"application/xml; charset=utf-8","svg"=>"image/svg+xml","png"=>"image/png","jpg"|"jpeg"=>"image/jpeg","gif"=>"image/gif","webp"=>"image/webp","avif"=>"image/avif","ico"=>"image/x-icon",_=>"application/octet-stream"};
    Response::builder().status(StatusCode::OK).header(header::CONTENT_TYPE,mime).header(header::CACHE_CONTROL,"no-store").body(Body::from(data)).unwrap_or_else(|_|StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
async fn validate_public_research_url(raw:&str)->Result<reqwest::Url,String>{
    use std::net::ToSocketAddrs;
    let url=reqwest::Url::parse(raw).map_err(|_|"URL inválida")?;
    if !["http","https"].contains(&url.scheme())||!url.username().is_empty()||url.password().is_some()||url.fragment().is_some(){return Err("A URL deve ser HTTP(S), sem credenciais nem fragmento.".into());}
    let host=url.host_str().ok_or("URL sem hostname")?.trim_end_matches('.').to_ascii_lowercase();
    if host=="localhost"||host.ends_with(".localhost")||host.ends_with(".local")||host.ends_with(".internal")||host.ends_with(".lan")||host.ends_with(".home"){return Err("Endereços locais não podem ser pesquisados.".into());}
    let port=url.port_or_known_default().ok_or("porta inválida")?;
    let owned_host=host.clone();
    let addrs=tokio::time::timeout(Duration::from_secs(3),tokio::task::spawn_blocking(move||(owned_host.as_str(),port).to_socket_addrs().map(|x|x.collect::<Vec<_>>()))).await.map_err(|_|"A resolução DNS expirou.")?.map_err(|_|"Falha de resolução DNS.")?.map_err(|_|"Não foi possível resolver o hostname.")?;
    if addrs.is_empty()||addrs.iter().any(|addr|!crate::tools::is_public_ip(addr.ip())){return Err("A URL resolve para um endereço não público; acesso bloqueado.".into());}
    Ok(url)
}
async fn fetch_public_html(raw:&str)->Result<(reqwest::Url,String),String>{
    let url=validate_public_research_url(raw).await?;
    let client=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(8)).build().map_err(|_|"Cliente HTTP indisponível.")?;
    let mut response=client.get(url.clone()).header(header::ACCEPT,"text/html,application/xhtml+xml").send().await.map_err(|_|"A página de pesquisa está indisponível.")?;
    if !response.status().is_success(){return Err(format!("A página respondeu HTTP {}.",response.status().as_u16()));}
    if response.headers().get(header::CONTENT_TYPE).and_then(|v|v.to_str().ok()).is_some_and(|v|!v.to_ascii_lowercase().contains("html")){return Err("A origem não é HTML.".into());}
    if response.content_length().is_some_and(|n|n>1_000_000){return Err("A página excede 1 MB.".into());}
    let mut bytes=Vec::new();
    while let Some(chunk)=response.chunk().await.map_err(|_|"Falha ao ler a página.")?{if bytes.len().saturating_add(chunk.len())>1_000_000{return Err("A página excede 1 MB.".into());}bytes.extend_from_slice(&chunk);}
    Ok((url,String::from_utf8_lossy(&bytes).into_owned()))
}
fn html_attr(tag:&str,name:&str)->String{
    let pattern=format!(r#"(?is)\b{}\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#,regex::escape(name));
    regex::Regex::new(&pattern).ok().and_then(|re|re.captures(tag).and_then(|c|(1..=3).find_map(|i|c.get(i).map(|m|m.as_str().to_owned())))).unwrap_or_default()
}
fn html_meta_images(html:&str,base:&reqwest::Url)->Vec<String>{
    let mut images=Vec::new();
    for tag in regex::Regex::new(r"(?is)<meta\b[^>]*>").unwrap().find_iter(html){
        let mut ident=html_attr(tag.as_str(),"property");if ident.is_empty(){ident=html_attr(tag.as_str(),"name");}
        if !["og:image","og:image:url","twitter:image","twitter:image:src"].contains(&ident.to_ascii_lowercase().as_str()){continue;}
        if let Ok(url)=base.join(&html_attr(tag.as_str(),"content")){let url=url.to_string();if !images.contains(&url){images.push(url);}}
        if images.len()>=6{break;}
    }
    images
}
async fn research_image(State(_state):State<Arc<AppState>>,Query(q):Query<std::collections::HashMap<String,String>>)->Response{
    let raw=q.get("url").map(String::as_str).unwrap_or("");
    if raw.is_empty(){return (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"url é obrigatório"}))).into_response();}
    match fetch_public_html(raw).await{
        Ok((base,html))=>{for url in html_meta_images(&html,&base){if validate_public_research_url(&url).await.is_ok(){return Json(json!({"ok":true,"image_url":url})).into_response();}}
            (StatusCode::NOT_FOUND,Json(json!({"ok":false,"image_url":""}))).into_response()},
        Err(error)=>(StatusCode::BAD_GATEWAY,Json(json!({"ok":false,"error":error,"image_url":""}))).into_response()
    }
}
async fn research_media(State(_state):State<Arc<AppState>>,Query(q):Query<std::collections::HashMap<String,String>>)->Response{
    let raw=q.get("url").map(String::as_str).unwrap_or("");
    if raw.is_empty(){return (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"url é obrigatório"}))).into_response();}
    match fetch_public_html(raw).await{
        Ok((base,html))=>{
            let mut images=Vec::new();for url in html_meta_images(&html,&base){if validate_public_research_url(&url).await.is_ok(){images.push(json!({"url":url,"title":"","caption":""}));}}
            let mut videos=Vec::new();
            for tag in regex::Regex::new(r#"(?is)<source\b[^>]*>"#).unwrap().find_iter(&html){
                let src=html_attr(tag.as_str(),"src");if src.is_empty(){continue;}
                let ty=html_attr(tag.as_str(),"type").to_ascii_lowercase();
                if let Ok(url)=base.join(&src){let value=url.to_string();if (ty.starts_with("video/")||value.to_ascii_lowercase().contains(".mp4"))&&validate_public_research_url(&value).await.is_ok(){videos.push(json!({"url":value,"thumbnailUrl":"","title":"","caption":""}));}}
                if videos.len()>=6{break;}
            }
            Json(json!({"ok":true,"source_url":base.as_str(),"images":images,"videos":videos})).into_response()
        },
        Err(error)=>(StatusCode::BAD_GATEWAY,Json(json!({"ok":false,"error":error,"source_url":"","images":[],"videos":[]}))).into_response()
    }
}
async fn research_images(State(state):State<Arc<AppState>>,Query(q):Query<std::collections::HashMap<String,String>>)->Response{
    let query=q.get("query").map(String::as_str).unwrap_or("").trim();
    if query.is_empty()||query.len()>240{return (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"query deve conter 1–240 bytes"}))).into_response();}
    let start=reqwest::Url::parse_with_params("https://duckduckgo.com/",&[("q",query),("iax","images"),("ia","images")]).unwrap();
    let html=match state.http.get(start).header(header::ACCEPT,"text/html").timeout(Duration::from_secs(8)).send().await{
        Ok(r) if r.status().is_success()=>match r.text().await{Ok(s)=>s.chars().take(300_000).collect::<String>(),Err(_)=>String::new()},
        _=>return (StatusCode::BAD_GATEWAY,Json(json!({"ok":false,"error":"provedor de busca de imagens indisponível","images":[]}))).into_response()
    };
    let token=regex::Regex::new(r#"(?i)vqd=["']([^"']+)["']"#).ok().and_then(|re|re.captures(&html).and_then(|c|c.get(1).map(|m|m.as_str().to_owned())));
    let Some(token)=token else{return (StatusCode::BAD_GATEWAY,Json(json!({"ok":false,"error":"token de busca não encontrado","images":[]}))).into_response()};
    let api=reqwest::Url::parse_with_params("https://duckduckgo.com/i.js",&[("l","us-en"),("o","json"),("q",query),("vqd",&token),("f",",,,"),("p","1")]).unwrap();
    let data=match state.http.get(api).header(header::ACCEPT,"application/json").timeout(Duration::from_secs(8)).send().await{
        Ok(r) if r.status().is_success()=>r.json::<Value>().await.unwrap_or(json!({})),
        _=>return (StatusCode::BAD_GATEWAY,Json(json!({"ok":false,"error":"resposta da busca de imagens inválida","images":[]}))).into_response()
    };
    let mut images=Vec::new();
    if let Some(items)=data["results"].as_array(){for item in items.iter().take(36){
        let image=item["image"].as_str().unwrap_or("");let page=item["url"].as_str().unwrap_or("");let thumb=item["thumbnail"].as_str().unwrap_or("");
        if image.is_empty()||validate_public_research_url(image).await.is_err(){continue;}
        if page.is_empty()||validate_public_research_url(page).await.is_err(){continue;}
        let thumbnail=if !thumb.is_empty()&&validate_public_research_url(thumb).await.is_ok(){thumb}else{image};
        images.push(json!({"image_url":image,"thumbnail_url":thumbnail,"title":item["title"].as_str().unwrap_or("").chars().take(240).collect::<String>(),"url":page}));
        if images.len()>=6{break;}
    }}
    Json(json!({"ok":true,"images":images})).into_response()
}

async fn context_metrics(State(state):State<Arc<AppState>>,Query(q):Query<std::collections::HashMap<String,String>>)->Response {
    let session=q.get("session_id").map(String::as_str).unwrap_or("").trim();
    if session.len()>160{return (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"session_id is limited to 160 bytes"}))).into_response();}
    let history=if session.is_empty(){Vec::new()}else{state.memory.history(session,100).unwrap_or_default()};
    let history_tokens=crate::context::estimate_tokens(&history);
    let mut instruction_chars=0usize;
    for name in ["AGENTS.md","PERSONALIDADE.md","CLAUDE.md"] {
        let path=state.config.workspace.join(name);
        if let Ok(text)=std::fs::read_to_string(path){instruction_chars=instruction_chars.saturating_add(text.chars().take(12_000).count());}
    }
    let names=if session.is_empty(){crate::tool_catalog::ready_names().into_iter().map(str::to_owned).collect()}else{state.active_names(session).await};
    let schemas=crate::tool_catalog::openai_tools(&names);
    let tool_tokens=crate::context::estimate_tokens(&schemas);
    let memory_count=state.memory.counts().ok().and_then(|v|v["memories"].as_u64()).unwrap_or(0);
    let settings=std::fs::read_to_string(state.config.data_dir.join("settings.json")).ok().and_then(|s|serde_json::from_str::<Value>(&s).ok()).unwrap_or_else(||json!({}));
    let model=settings["thomasModel"].as_str().or_else(||settings["model"].as_str()).unwrap_or("");
    let (compact,window)=crate::agent::context_policy(&state.config.data_dir,model);
    let instruction_tokens=(instruction_chars+3)/4;
    let system_base_tokens=900usize;
    let estimated=system_base_tokens.saturating_add(instruction_tokens).saturating_add(tool_tokens).saturating_add(history_tokens);
    Json(json!({
        "ok":true,
        "session_id":session,
        "compact_enabled":compact,
        "context_window_tokens":window,
        "soft_limit_tokens":window*70/100,
        "hard_limit_tokens":window*90/100,
        "estimated_tokens":estimated,
        "sections":{
            "system_base_tokens":{"estimate":system_base_tokens,"source":"built-in system instructions"},
            "project_instructions_tokens":{"estimate":instruction_tokens,"source":"AGENTS.md, PERSONALIDADE.md, CLAUDE.md"},
            "active_tool_schemas_tokens":{"estimate":tool_tokens,"source":"active tool JSON schemas"},
            "persisted_history_tokens":{"estimate":history_tokens,"source":"SQLite session history"}
        },
        "available_long_term_memories":memory_count,
        "note":"Estimates use serialized characters / 4; provider-reported usage is authoritative. Plugin, request and active-turn observations may add tokens."
    })).into_response()
}
