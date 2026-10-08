use super::*;

#[test]
fn canvas_patterns_native_and_clipped_batches_match_independent_scalar_rows() {
    let source = include_str!("../../../../tests/canvas/pattern-paint.js");
    let (_, outcome) = execute_html(&format!(
        "<script>{source}\ntestPatternPainting(()=>document.createElement('canvas'));</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn canvas_and_path_transforms_share_exact_dictionary_and_numeric_conversion() {
    let source = include_str!("../../../../tests/canvas/matrix-contracts.js");
    let (_, outcome) = execute_html(&format!(
        "<script>{source}\ntestMatrixContracts(()=>document.createElement('canvas'));</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn patterns_preserve_opaque_snapshots_and_convert_only_matrix_dictionary_members() {
    let source = include_str!("../../../../tests/canvas/pattern-contracts.js");
    let (_, outcome) = execute_html(&format!(
        "<script>{source}\ntestPatternContracts(()=>document.createElement('canvas'));</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn pattern_receiver_brand_survives_prototype_changes_and_weakmap_replacement() {
    let (_, outcome) = execute_html(
        r#"<script>
    const source=document.createElement('canvas');source.width=2;source.height=1;
    const s=source.getContext('2d');s.fillStyle='red';s.fillRect(0,0,2,1);
    const target=document.createElement('canvas');target.width=4;target.height=1;
    const c=target.getContext('2d'),pattern=c.createPattern(source,'repeat');
    const setter=CanvasPattern.prototype.setTransform;
    Object.setPrototypeOf(pattern,null);c.fillStyle=pattern;c.strokeStyle=pattern;
    if(c.fillStyle!==pattern||c.strokeStyle!==pattern)throw Error('implementation brand');
    const get=WeakMap.prototype.get,has=WeakMap.prototype.has,set=WeakMap.prototype.set;
    try {
        WeakMap.prototype.get=WeakMap.prototype.has=WeakMap.prototype.set=()=>{throw Error('author weak hook');};
        setter.call(pattern,{e:1});
    } finally { WeakMap.prototype.get=get;WeakMap.prototype.has=has;WeakMap.prototype.set=set; }
    c.fillRect(0,0,4,1);if(c.getImageData(0,0,1,1).data[0]!==255)throw Error('genuine brand painting');
    const fake=Object.create(CanvasPattern.prototype);let failed=false;
    try{setter.call(fake);}catch(error){failed=error instanceof TypeError;}
    if(!failed)throw Error('forged receiver');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn pattern_image_conversion_precedes_string_and_snapshot_follows_it() {
    let (_, outcome) = execute_html(
        r#"<script>
    const c=document.createElement('canvas').getContext('2d');let calls=0;
    const repetition={toString(){calls++;return 'repeat';}};
    try{c.createPattern({},repetition);}catch(error){if(!(error instanceof TypeError))throw error;}
    if(calls!==0)throw Error('invalid interface converted repetition');
    const source=document.createElement('canvas');source.width=1;source.height=1;
    const s=source.getContext('2d');s.fillStyle='red';s.fillRect(0,0,1,1);
    const pattern=c.createPattern(source,{toString(){s.fillStyle='blue';s.fillRect(0,0,1,1);return 'repeat';}});
    c.fillStyle=pattern;c.fillRect(0,0,1,1);
    if(c.getImageData(0,0,1,1).data[2]!==255)throw Error('snapshot before argument conversion');
    let read=0;const matrix={get a(){read++;return {valueOf(){read++;return 1;}};}};
    pattern.setTransform(matrix);if(read!==2)throw Error('member or number converted twice');
    pattern.setTransform({a:NaN,m11:NaN});
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
