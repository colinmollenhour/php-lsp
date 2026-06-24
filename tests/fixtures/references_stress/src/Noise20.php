<?php

namespace App;

class Noise20
{
    private function compute(): int
    {
        return 20;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
