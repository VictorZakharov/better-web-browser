use super::*;

#[test]
fn progress_scale_transitions_have_a_real_intermediate_fill_and_preserve_author_style() {
    let (dom, outcome) = execute_html(
        r#"<style>
      #fill{transform:scaleX(.04);transform-origin:left;transition:transform 1s linear}
      #fill.on{transform:scaleX(1)}
    </style><div id=fill></div><output></output><script>
      let now=100;performance.now=()=>now;
      const fill=document.getElementById('fill');fill.classList.add('on');now=600;
      requestAnimationFrame(()=>{
        const transform=getComputedStyle(fill).transform;
        if(!transform.includes('0.52'))throw Error('not intermediate '+transform);
        if(fill.getAttribute('style')!==null)throw Error('rewritten author style');
        now=1100;requestAnimationFrame(()=>{
          if(getComputedStyle(fill).transform!=='scale(1, 1)')throw Error('final scale');
          document.querySelector('output').textContent='yes';
        });
      });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn individual_rotation_scale_and_inset_clips_are_sampled_not_discrete() {
    let (_, outcome) = execute_html(
        r#"<style>
      #target{rotate:0deg;scale:1;clip-path:inset(-50% -10% -50% -50%);
        transition:rotate 1s linear,scale 1s linear,clip-path 1s linear}
      #target.on{rotate:90deg;scale:2 3;clip-path:inset(-50% 150% -50% -50%)}
    </style><div id=target></div><script>
      let now=100;performance.now=()=>now;
      const target=document.getElementById('target');target.classList.add('on');now=600;
      requestAnimationFrame(()=>{
        const style=getComputedStyle(target);
        if(Math.abs(parseFloat(style.rotate)-45)>0.001)throw Error('rotate '+style.rotate);
        if(style.scale!=='1.5 2')throw Error('scale '+style.scale);
        if(style.clipPath.includes('150%')||style.clipPath==='none')throw Error('discrete clip '+style.clipPath);
      });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
